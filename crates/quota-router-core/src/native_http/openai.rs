// openai — OpenAI via reqwest (native_http, LiteLLM mode)

use super::{HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse, ProviderError, StreamingResponse};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;

pub struct OpenAIProvider {
    client: Client,
    api_base: String,
}

impl OpenAIProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.openai.com/v1".to_string(),
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }
}

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "gpt-4", "gpt-4-turbo", "gpt-4o", "gpt-4o-mini",
            "gpt-3.5-turbo", "gpt-4-0613", "gpt-4-32k",
        ]
    }

    fn supports_streaming(&self) -> bool {
        true
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

        if let Some(stream) = request.stream {
            body["stream"] = serde_json::json!(stream);
        }
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
        if let Some(n) = request.n {
            body["n"] = serde_json::json!(n);
        }
        if let Some(p) = request.presence_penalty {
            body["presence_penalty"] = serde_json::json!(p);
        }
        if let Some(p) = request.frequency_penalty {
            body["frequency_penalty"] = serde_json::json!(p);
        }
        if let Some(user) = &request.user {
            body["user"] = serde_json::json!(user);
        }

        let resp = self.client
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
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        let status = resp.status();
        let data: OpenAIResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(convert_response(data, status.as_u16()))
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

        let data: OpenAIEmbeddingsResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

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
        10 // Higher weight for OpenAI as primary provider
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<StreamingResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.api_base);

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

        let resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
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
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        // For OpenAI, we pass raw SSE bytes through
        // The proxy will forward SSE bytes directly to the client
        let (tx, rx) = mpsc::channel(100);

        // Spawn task to read SSE bytes and forward them
        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        // Send raw SSE bytes to proxy for direct forwarding
                        if tx.send(Ok(super::StreamingChunk::RawSSE(bytes.to_vec()))).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(ProviderError::Network(e.to_string()))).await;
                        break;
                    }
                }
            }
        });

        Ok(StreamingResponse {
            receiver: rx,
            content_type: "text/event-stream",
        })
    }
}

#[derive(Deserialize)]
struct OpenAIResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: OpenAIUsage,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    index: u32,
    message: OpenAIMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OpenAIEmbeddingsResponse {
    object: String,
    data: Vec<OpenAIEmbedding>,
    model: String,
    usage: OpenAIUsage,
}

#[derive(Deserialize)]
struct OpenAIEmbedding {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}

fn convert_response(data: OpenAIResponse, _status: u16) -> HttpCompletionResponse {
    let choices = data.choices.into_iter().map(|c| {
        crate::shared_types::Choice::new(
            c.index,
            crate::shared_types::Message::new(c.message.role, c.message.content),
            c.finish_reason,
        )
    }).collect();

    HttpCompletionResponse {
        id: data.id,
        object: data.object,
        created: data.created,
        model: data.model,
        choices,
        usage: crate::shared_types::Usage::new(data.usage.prompt_tokens, data.usage.completion_tokens, data.usage.total_tokens),
    }
}
