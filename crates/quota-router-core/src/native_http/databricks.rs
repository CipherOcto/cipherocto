// databricks — Databricks via reqwest (native_http, LiteLLM mode)

use super::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingResponse,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;

pub struct DatabricksProvider {
    client: Client,
    api_base: String,
}

impl DatabricksProvider {
    pub fn new() -> Self {
        let api_base = std::env::var("DATABRICKS_BASE_URL")
            .unwrap_or_else(|_| "https://dbc-xxx.databricks.com".to_string());
        Self {
            client: Client::new(),
            api_base: Self::validate_url(&api_base).unwrap_or(api_base),
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = Self::validate_url(&api_base).unwrap_or(api_base);
        self
    }

    /// Validate workspace URL — HTTPS only per security requirements
    fn validate_url(url: &str) -> Option<String> {
        if url.starts_with("https://") {
            Some(url.to_string())
        } else if url.starts_with("http://") {
            // Upgrade to HTTPS
            Some(url.replacen("http://", "https://", 1))
        } else {
            // Invalid URL, keep original but log warning
            None
        }
    }

    /// Strip the "databricks/" prefix from model name
    fn strip_model_prefix(model: &str) -> &str {
        model.strip_prefix("databricks/").unwrap_or(model)
    }
}

impl Default for DatabricksProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for DatabricksProvider {
    fn name(&self) -> &str {
        "databricks"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec!["databricks/"]
    }

    fn supports_model(&self, model: &str) -> bool {
        model.starts_with("databricks/")
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        // Use api_base from request if provided, otherwise fall back to provider's default
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let endpoint = Self::strip_model_prefix(&request.model);
        let url = format!("{}/serving-endpoints/{}/invocations", base_url, endpoint);

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

        // Function calling fields
        if let Some(tools) = &request.tools {
            body["tools"] = serde_json::to_value(tools).unwrap_or_default();
        }
        if let Some(tool_choice) = &request.tool_choice {
            body["tool_choice"] = serde_json::to_value(tool_choice).unwrap_or_default();
        }
        if let Some(fmt) = &request.response_format {
            body["response_format"] = serde_json::to_value(fmt).unwrap_or_default();
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

        let status = resp.status();
        let data: DatabricksResponse = resp
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
        let endpoint = Self::strip_model_prefix(&request.model);
        let url = format!("{}/serving-endpoints/{}/invocations", base_url, endpoint);

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

        let data: DatabricksEmbeddingsResponse = resp
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
        5 // Lower weight than OpenAI
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<StreamingResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let endpoint = Self::strip_model_prefix(&request.model);
        let url = format!("{}/serving-endpoints/{}/invocations", base_url, endpoint);

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

        // Databricks uses OpenAI-compatible SSE format
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
struct DatabricksResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<DatabricksChoice>,
    usage: DatabricksUsage,
}

#[derive(Deserialize)]
struct DatabricksChoice {
    index: u32,
    message: DatabricksMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct DatabricksMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct DatabricksUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct DatabricksEmbeddingsResponse {
    object: String,
    data: Vec<DatabricksEmbedding>,
    model: String,
    usage: DatabricksUsage,
}

#[derive(Deserialize)]
struct DatabricksEmbedding {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}

fn convert_response(data: DatabricksResponse, _status: u16) -> HttpCompletionResponse {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_http::HttpProvider;

    #[test]
    fn test_strip_model_prefix() {
        assert_eq!(
            DatabricksProvider::strip_model_prefix("databricks/dbrx-instruct"),
            "dbrx-instruct"
        );
        assert_eq!(
            DatabricksProvider::strip_model_prefix("dbrx-instruct"),
            "dbrx-instruct"
        );
        assert_eq!(
            DatabricksProvider::strip_model_prefix("databricks/llama-3-70b"),
            "llama-3-70b"
        );
    }

    #[test]
    fn test_validate_url_https() {
        assert_eq!(
            DatabricksProvider::validate_url("https://dbc-xxx.databricks.com"),
            Some("https://dbc-xxx.databricks.com".to_string())
        );
    }

    #[test]
    fn test_validate_url_http_upgrade() {
        assert_eq!(
            DatabricksProvider::validate_url("http://dbc-xxx.databricks.com"),
            Some("https://dbc-xxx.databricks.com".to_string())
        );
    }

    #[test]
    fn test_validate_url_invalid() {
        assert_eq!(DatabricksProvider::validate_url("ftp://invalid"), None);
    }

    #[test]
    fn test_provider_name() {
        let provider = DatabricksProvider::new();
        assert_eq!(provider.name(), "databricks");
    }

    #[test]
    fn test_supported_models() {
        let provider = DatabricksProvider::new();
        let models = provider.supported_models();
        assert!(models.contains(&"databricks/"));
    }

    #[test]
    fn test_supports_streaming() {
        let provider = DatabricksProvider::new();
        assert!(provider.supports_streaming());
    }

    #[test]
    fn test_convert_response() {
        let data = DatabricksResponse {
            id: "test-id".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "dbrx-instruct".to_string(),
            choices: vec![DatabricksChoice {
                index: 0,
                message: DatabricksMessage {
                    role: "assistant".to_string(),
                    content: "Hello!".to_string(),
                },
                finish_reason: "stop".to_string(),
            }],
            usage: DatabricksUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        };

        let response = convert_response(data, 200);
        assert_eq!(response.id, "test-id");
        assert_eq!(response.model, "dbrx-instruct");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello!".to_string())
        );
        assert_eq!(response.usage.total_tokens, 15);
    }
}
