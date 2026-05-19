// perplexity — Perplexity via reqwest (native_http, LiteLLM mode)

use super::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingResponse,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

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
        self.api_base = Self::validate_url(&api_base).unwrap_or_else(|| {
            eprintln!(
                "WARNING: Invalid Perplexity URL '{}', using default",
                api_base
            );
            "https://api.perplexity.ai".to_string()
        });
        self
    }

    /// Validate API base URL — HTTPS only per security requirements
    fn validate_url(url: &str) -> Option<String> {
        if url.starts_with("https://") {
            Some(url.to_string())
        } else if url.starts_with("http://") {
            // Upgrade to HTTPS
            Some(url.replacen("http://", "https://", 1))
        } else {
            // Invalid URL
            None
        }
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
        api_key: Option<&str>,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/chat/completions", base_url);

        let model = Self::strip_model_prefix(&request.model);
        let mut body = super::build_openai_compatible_body(request, model);

        // Merge Perplexity-specific params from provider_params
        if let Some(params) = &request.provider_params {
            if let Some(obj) = params.as_object() {
                for (key, value) in obj {
                    // return_citations, search_domain_filter, search_recency_filter
                    if matches!(
                        key.as_str(),
                        "return_citations"
                            | "search_domain_filter"
                            | "search_recency_filter"
                            | "return_images"
                            | "return_related_questions"
                    ) {
                        body[key] = value.clone();
                    }
                }
            }
        }

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
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
        api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/embeddings", base_url);

        let model = Self::strip_model_prefix(&request.model);

        let body = serde_json::json!({
            "input": request.input,
            "model": model
        });

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
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
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/chat/completions", base_url);

        let model = Self::strip_model_prefix(&request.model);
        let mut body = super::build_openai_compatible_body(request, model);
        body["stream"] = serde_json::json!(true);

        // Merge Perplexity-specific params from provider_params
        if let Some(params) = &request.provider_params {
            if let Some(obj) = params.as_object() {
                for (key, value) in obj {
                    if matches!(
                        key.as_str(),
                        "return_citations"
                            | "search_domain_filter"
                            | "search_recency_filter"
                            | "return_images"
                            | "return_related_questions"
                    ) {
                        body[key] = value.clone();
                    }
                }
            }
        }

        super::stream_openai_compatible(&self.client, &url, api_key, body).await
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

    #[test]
    fn test_convert_response_empty_choices() {
        let data = PerplexityResponse {
            id: "empty-id".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "sonar-large-online".to_string(),
            choices: vec![],
            usage: PerplexityUsage {
                prompt_tokens: 10,
                completion_tokens: 0,
                total_tokens: 10,
            },
            citations: None,
        };

        let response = convert_response(data, 200);
        assert_eq!(response.id, "empty-id");
        assert_eq!(response.model, "sonar-large-online");
        assert!(response.choices.is_empty());
        assert_eq!(response.usage.total_tokens, 10);
        assert!(response.metadata.is_none());
    }

    #[test]
    fn test_convert_response_error_status() {
        let data = PerplexityResponse {
            id: "error-id".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "sonar-large-online".to_string(),
            choices: vec![PerplexityChoice {
                index: 0,
                message: PerplexityMessage {
                    role: "assistant".to_string(),
                    content: "Error occurred".to_string(),
                },
                finish_reason: "stop".to_string(),
            }],
            usage: PerplexityUsage {
                prompt_tokens: 5,
                completion_tokens: 3,
                total_tokens: 8,
            },
            citations: None,
        };

        // convert_response ignores status; verify it still produces a valid response
        let response = convert_response(data, 500);
        assert_eq!(response.id, "error-id");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            Some("Error occurred".to_string())
        );
        assert_eq!(response.usage.total_tokens, 8);
    }

    #[test]
    fn test_convert_response_without_citations() {
        let data = PerplexityResponse {
            id: "no-cite-id".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "sonar-small-online".to_string(),
            choices: vec![PerplexityChoice {
                index: 0,
                message: PerplexityMessage {
                    role: "assistant".to_string(),
                    content: "No citations".to_string(),
                },
                finish_reason: "stop".to_string(),
            }],
            usage: PerplexityUsage {
                prompt_tokens: 8,
                completion_tokens: 4,
                total_tokens: 12,
            },
            citations: None,
        };

        let response = convert_response(data, 200);
        assert_eq!(response.id, "no-cite-id");
        assert_eq!(response.model, "sonar-small-online");
        assert_eq!(response.choices.len(), 1);
        assert!(response.metadata.is_none());
    }

    #[test]
    fn test_validate_url_https() {
        assert_eq!(
            PerplexityProvider::validate_url("https://api.perplexity.ai"),
            Some("https://api.perplexity.ai".to_string())
        );
    }

    #[test]
    fn test_validate_url_http_upgrade() {
        assert_eq!(
            PerplexityProvider::validate_url("http://api.perplexity.ai"),
            Some("https://api.perplexity.ai".to_string())
        );
    }

    #[test]
    fn test_validate_url_invalid() {
        assert_eq!(PerplexityProvider::validate_url("ftp://invalid"), None);
    }

    #[test]
    fn test_with_api_base_invalid_url() {
        let provider = PerplexityProvider::new().with_api_base("ftp://not-a-url".to_string());
        // Should fall back to default
        assert_eq!(provider.api_base, "https://api.perplexity.ai");
    }

    #[test]
    fn test_with_api_base_valid_url() {
        let provider =
            PerplexityProvider::new().with_api_base("https://custom.perplexity.ai".to_string());
        assert_eq!(provider.api_base, "https://custom.perplexity.ai");
    }
}
