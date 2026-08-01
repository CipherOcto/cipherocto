// databricks — Databricks via reqwest (native_http, LiteLLM mode)

use crate::native_http::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingResponse,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

pub struct DatabricksProvider {
    client: Client,
    api_base: String,
}

impl DatabricksProvider {
    pub fn new() -> Self {
        let api_base = std::env::var("DATABRICKS_BASE_URL")
            .unwrap_or_else(|_| "https://dbc-xxx.databricks.com".to_string());
        let validated = Self::validate_url(&api_base).unwrap_or_else(|| {
            eprintln!(
                "WARNING: Invalid DATABRICKS_BASE_URL '{}', using default",
                api_base
            );
            "https://dbc-xxx.databricks.com".to_string()
        });
        Self {
            client: Client::new(),
            api_base: validated,
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = Self::validate_url(&api_base).unwrap_or_else(|| {
            eprintln!(
                "WARNING: Invalid Databricks URL '{}', using default",
                api_base
            );
            "https://dbc-xxx.databricks.com".to_string()
        });
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
        api_key: Option<&str>,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        // Use api_base from request if provided, otherwise fall back to provider's default
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let endpoint = Self::strip_model_prefix(&request.model);
        let url = format!("{}/serving-endpoints/{}/invocations", base_url, endpoint);

        let model = Self::strip_model_prefix(&request.model);
        let body = super::build_openai_compatible_body(request, model);

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            let bearer = crate::egress::key_swap::attach_bearer(&key)
                .expect("provider-boundary key-swap: api_key MUST be provider-shaped; if this fires, the upstream source path leaked a CipherOcto key");
            req_builder = req_builder.header("Authorization", bearer);
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
        let data: DatabricksResponse = resp
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
        let endpoint = Self::strip_model_prefix(&request.model);
        let url = format!("{}/serving-endpoints/{}/invocations", base_url, endpoint);

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
            let bearer = crate::egress::key_swap::attach_bearer(&key)
                .expect("provider-boundary key-swap: api_key MUST be provider-shaped; if this fires, the upstream source path leaked a CipherOcto key");
            req_builder = req_builder.header("Authorization", bearer);
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
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let endpoint = Self::strip_model_prefix(&request.model);
        let url = format!("{}/serving-endpoints/{}/invocations", base_url, endpoint);

        let model = Self::strip_model_prefix(&request.model);
        let mut body = super::build_openai_compatible_body(request, model);
        body["stream"] = serde_json::json!(true);

        super::stream_openai_compatible(&self.client, &url, api_key, body).await
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
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_http::HttpBatchCreateRequest;
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

    #[test]
    fn test_convert_response_empty_choices() {
        let data = DatabricksResponse {
            id: "empty-id".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "dbrx-instruct".to_string(),
            choices: vec![],
            usage: DatabricksUsage {
                prompt_tokens: 10,
                completion_tokens: 0,
                total_tokens: 10,
            },
        };

        let response = convert_response(data, 200);
        assert_eq!(response.id, "empty-id");
        assert_eq!(response.model, "dbrx-instruct");
        assert!(response.choices.is_empty());
        assert_eq!(response.usage.total_tokens, 10);
    }

    #[test]
    fn test_convert_response_error_status() {
        let data = DatabricksResponse {
            id: "error-id".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "dbrx-instruct".to_string(),
            choices: vec![DatabricksChoice {
                index: 0,
                message: DatabricksMessage {
                    role: "assistant".to_string(),
                    content: "Error occurred".to_string(),
                },
                finish_reason: "stop".to_string(),
            }],
            usage: DatabricksUsage {
                prompt_tokens: 5,
                completion_tokens: 3,
                total_tokens: 8,
            },
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
    fn test_with_api_base_invalid_url() {
        let provider = DatabricksProvider::new().with_api_base("ftp://not-a-url".to_string());
        // Should fall back to default
        assert_eq!(provider.api_base, "https://dbc-xxx.databricks.com");
    }

    #[test]
    fn test_with_api_base_valid_url() {
        let provider =
            DatabricksProvider::new().with_api_base("https://custom.databricks.com".to_string());
        assert_eq!(provider.api_base, "https://custom.databricks.com");
    }

    #[test]
    fn test_supports_model() {
        let p = DatabricksProvider::new();
        assert!(p.supports_model("databricks/dbrx-instruct"));
        assert!(!p.supports_model("gpt-4"));
    }

    #[test]
    fn test_default_trait_methods() {
        let p = DatabricksProvider::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(p.get_response("id", None, None, None)).is_err());
        assert!(rt
            .block_on(p.delete_response("id", None, None, None))
            .is_err());
        let batch_req = HttpBatchCreateRequest {
            input_file: "f".into(),
            endpoint: "/v1".into(),
            completion_window: "24h".into(),
            metadata: None,
            api_base: None,
            timeout: None,
        };
        assert!(rt.block_on(p.batch_create(&batch_req, None)).is_err());
        assert!(rt
            .block_on(p.batch_retrieve("id", None, None, None))
            .is_err());
        assert!(rt.block_on(p.batch_cancel("id", None, None, None)).is_err());
        assert!(rt.block_on(p.batch_list(None, None, None, None)).is_err());
        assert!(rt
            .block_on(p.batch_results("id", None, None, None))
            .is_err());
        assert!(rt.block_on(p.list_models(None, None, None)).is_err());
    }

    #[tokio::test]
    async fn completion_network_error() {
        let p = DatabricksProvider::new().with_api_base("https://dbc-xxx.databricks.com".into());
        let req = HttpCompletionRequest {
            model: "databricks/dbrx-instruct".into(),
            messages: vec![crate::shared_types::Message {
                role: "user".into(),
                content: Some("hi".into()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                function_call: None,
            }],
            stream: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            n: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            api_base: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            parallel_tool_calls: None,
            prompt_id: None,
            prompt_variables: None,
            provider_params: None,
            timeout: None,
        };
        assert!(p.completion(&req, None).await.is_err());
    }

    #[tokio::test]
    async fn completion_auth_401() {
        let s = crate::testing::mock_http::MockHttpServer::unauthorized().await;
        let p = DatabricksProvider::new().with_api_base(s.base_url());
        let req = HttpCompletionRequest {
            model: "databricks/dbrx-instruct".into(),
            messages: vec![crate::shared_types::Message {
                role: "user".into(),
                content: Some("hi".into()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                function_call: None,
            }],
            stream: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            n: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            api_base: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            parallel_tool_calls: None,
            prompt_id: None,
            prompt_variables: None,
            provider_params: None,
            timeout: None,
        };
        assert!(p.completion(&req, Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn completion_rate_limit() {
        let s = crate::testing::mock_http::MockHttpServer::rate_limited().await;
        let p = DatabricksProvider::new().with_api_base(s.base_url());
        let req = HttpCompletionRequest {
            model: "databricks/dbrx-instruct".into(),
            messages: vec![crate::shared_types::Message {
                role: "user".into(),
                content: Some("hi".into()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                function_call: None,
            }],
            stream: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            n: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            api_base: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            parallel_tool_calls: None,
            prompt_id: None,
            prompt_variables: None,
            provider_params: None,
            timeout: None,
        };
        assert!(p.completion(&req, Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn completion_server_error() {
        let s = crate::testing::mock_http::MockHttpServer::error().await;
        let p = DatabricksProvider::new().with_api_base(s.base_url());
        let req = HttpCompletionRequest {
            model: "databricks/dbrx-instruct".into(),
            messages: vec![crate::shared_types::Message {
                role: "user".into(),
                content: Some("hi".into()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                function_call: None,
            }],
            stream: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            n: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            api_base: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            parallel_tool_calls: None,
            prompt_id: None,
            prompt_variables: None,
            provider_params: None,
            timeout: None,
        };
        assert!(p.completion(&req, Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn embedding_network_error() {
        let p = DatabricksProvider::new().with_api_base("https://dbc-xxx.databricks.com".into());
        assert!(p
            .embedding(
                &HttpEmbeddingRequest {
                    input: "t".into(),
                    model: "m".into(),
                    api_base: None,
                    timeout: None,
                },
                None,
            )
            .await
            .is_err());
    }
}
