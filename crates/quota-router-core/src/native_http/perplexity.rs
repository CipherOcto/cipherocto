// perplexity — Perplexity via reqwest (native_http, LiteLLM mode)

use super::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingResponse,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;

pub struct PerplexityProvider {
    client: Client,
    api_base: String,
}

impl PerplexityProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.perplexity.ai".to_string(),
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    /// Strip the "perplexity/" prefix from model name
    fn strip_model_prefix(model: &str) -> &str {
        model.strip_prefix("perplexity/").unwrap_or(model)
    }
}

impl Default for PerplexityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for PerplexityProvider {
    fn name(&self) -> &str {
        "perplexity"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "perplexity/sonar-small-online",
            "perplexity/sonar-medium-online",
            "perplexity/sonar-large-online",
            "perplexity/",
        ]
    }

    fn supports_model(&self, model: &str) -> bool {
        model.starts_with("perplexity/")
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/chat/completions", base_url);

        let model = Self::strip_model_prefix(&request.model);

        let mut body = serde_json::json!({
            "model": model,
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
        if let Some(penalty) = request.presence_penalty {
            body["presence_penalty"] = serde_json::json!(penalty);
        }
        if let Some(penalty) = request.frequency_penalty {
            body["frequency_penalty"] = serde_json::json!(penalty);
        }
        if let Some(user) = &request.user {
            body["user"] = serde_json::json!(user);
        }
        if let Some(seed) = request.seed {
            body["seed"] = serde_json::json!(seed);
        }
        if let Some(tools) = &request.tools {
            body["tools"] = serde_json::to_value(tools).unwrap_or_default();
        }
        if let Some(tool_choice) = &request.tool_choice {
            body["tool_choice"] = serde_json::to_value(tool_choice).unwrap_or_default();
        }
        if let Some(fmt) = &request.response_format {
            body["response_format"] = serde_json::to_value(fmt).unwrap_or_default();
        }

        // Note: Perplexity-specific fields (return_citations, search_domain_filter,
        // search_recency_filter) are not supported through the standard
        // HttpCompletionRequest interface. Use the Python SDK for these features.

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

        let status = resp.status();
        let data: PerplexityResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(convert_response(data, status.as_u16()))
    }

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: &str,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/embeddings", base_url);

        let model = Self::strip_model_prefix(&request.model);

        let body = serde_json::json!({
            "input": request.input,
            "model": model
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

        let data: PerplexityEmbeddingsResponse = resp
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

    fn routing_weight(&self) -> u32 {
        5
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<StreamingResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/chat/completions", base_url);

        let model = Self::strip_model_prefix(&request.model);

        let mut body = serde_json::json!({
            "model": model,
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
        if let Some(tools) = &request.tools {
            body["tools"] = serde_json::to_value(tools).unwrap_or_default();
        }
        if let Some(tool_choice) = &request.tool_choice {
            body["tool_choice"] = serde_json::to_value(tool_choice).unwrap_or_default();
        }
        if let Some(fmt) = &request.response_format {
            body["response_format"] = serde_json::to_value(fmt).unwrap_or_default();
        }
        if let Some(n) = request.n {
            body["n"] = serde_json::json!(n);
        }
        if let Some(penalty) = request.presence_penalty {
            body["presence_penalty"] = serde_json::json!(penalty);
        }
        if let Some(penalty) = request.frequency_penalty {
            body["frequency_penalty"] = serde_json::json!(penalty);
        }
        if let Some(user) = &request.user {
            body["user"] = serde_json::json!(user);
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

        // Perplexity uses OpenAI-compatible SSE format
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        if tx
                            .send(Ok(super::StreamingChunk::RawSSE(bytes.to_vec())))
                            .await
                            .is_err()
                        {
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
struct PerplexityResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<PerplexityChoice>,
    usage: PerplexityUsage,
    #[serde(default)]
    #[allow(dead_code)]
    citations: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct PerplexityChoice {
    index: u32,
    message: PerplexityMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct PerplexityMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct PerplexityUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct PerplexityEmbeddingsResponse {
    object: String,
    data: Vec<PerplexityEmbedding>,
    model: String,
    usage: PerplexityUsage,
}

#[derive(Deserialize)]
struct PerplexityEmbedding {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}

fn convert_response(data: PerplexityResponse, _status: u16) -> HttpCompletionResponse {
    let choices = data
        .choices
        .into_iter()
        .map(|c| {
            crate::shared_types::Choice::new(
                c.index,
                crate::shared_types::Message::new(c.message.role, c.message.content),
                c.finish_reason,
            )
        })
        .collect();

    // Preserve Perplexity-specific citations in metadata
    let metadata = data
        .citations
        .map(|citations| serde_json::json!({ "citations": citations }));

    HttpCompletionResponse {
        id: data.id,
        object: data.object,
        created: data.created,
        model: data.model,
        choices,
        usage: crate::shared_types::Usage::new(
            data.usage.prompt_tokens,
            data.usage.completion_tokens,
            data.usage.total_tokens,
        ),
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_http::HttpProvider;

    #[test]
    fn test_strip_model_prefix() {
        assert_eq!(
            PerplexityProvider::strip_model_prefix("perplexity/sonar-small-online"),
            "sonar-small-online"
        );
        assert_eq!(
            PerplexityProvider::strip_model_prefix("sonar-small-online"),
            "sonar-small-online"
        );
    }

    #[test]
    fn test_provider_name() {
        let provider = PerplexityProvider::new();
        assert_eq!(provider.name(), "perplexity");
    }

    #[test]
    fn test_supported_models() {
        let provider = PerplexityProvider::new();
        let models = provider.supported_models();
        assert!(models.contains(&"perplexity/sonar-small-online"));
        assert!(models.contains(&"perplexity/sonar-medium-online"));
        assert!(models.contains(&"perplexity/sonar-large-online"));
        assert!(models.contains(&"perplexity/"));
    }

    #[test]
    fn test_supports_streaming() {
        let provider = PerplexityProvider::new();
        assert!(provider.supports_streaming());
    }

    #[test]
    fn test_convert_response() {
        let data = PerplexityResponse {
            id: "test-id".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "sonar-large-online".to_string(),
            choices: vec![PerplexityChoice {
                index: 0,
                message: PerplexityMessage {
                    role: "assistant".to_string(),
                    content: "Hello!".to_string(),
                },
                finish_reason: "stop".to_string(),
            }],
            usage: PerplexityUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            citations: Some(vec!["https://example.com".to_string()]),
        };

        let response = convert_response(data, 200);
        assert_eq!(response.id, "test-id");
        assert_eq!(response.model, "sonar-large-online");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello!".to_string())
        );
        assert_eq!(response.usage.total_tokens, 15);
        // Verify citations are preserved in metadata
        assert!(response.metadata.is_some());
        let meta = response.metadata.unwrap();
        assert_eq!(meta["citations"][0], "https://example.com");
    }
}
