// azure — Azure OpenAI via reqwest (native_http, LiteLLM mode)

use super::{
    HttpBatchCreateRequest, HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest,
    HttpEmbeddingResponse, ProviderError, StreamingChunk, StreamingResponse,
};
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
            "gpt-4",
            "gpt-4-turbo",
            "gpt-4o",
            "gpt-35-turbo",
            "gpt-35-turbo-16k",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let deployment = request.model.clone();
        let url = format!(
            "{}/openai/deployments/{}/chat/completions?api-version=2024-02-01",
            self.api_base, deployment
        );

        let mut body = serde_json::json!({
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

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("api-key", key);
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

        let data: AzureResponse = resp
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
            metadata: None,
        })
    }

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        let deployment = request.model.clone();
        let url = format!(
            "{}/openai/deployments/{}/embeddings?api-version=2024-02-01",
            self.api_base, deployment
        );

        let body = serde_json::json!({
            "input": request.input
        });

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("api-key", key);
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

        let data: AzureEmbeddingsResponse = resp
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

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let deployment = request.model.clone();
        let url = format!(
            "{}/openai/deployments/{}/chat/completions?api-version=2024-02-01",
            base_url, deployment
        );

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_http::HttpProvider;
    use crate::testing::mock_http::MockHttpServer;

    fn msg(role: &str, c: &str) -> crate::shared_types::Message {
        crate::shared_types::Message {
            role: role.into(),
            content: Some(c.into()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            function_call: None,
        }
    }

    fn req(model: &str) -> HttpCompletionRequest {
        HttpCompletionRequest {
            model: model.into(),
            messages: vec![msg("user", "hi")],
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
        }
    }

    fn ok_response() -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "Hi!"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        })
    }

    fn ok_embeddings() -> serde_json::Value {
        serde_json::json!({
            "object": "list",
            "data": [{"object": "embedding", "embedding": [0.1, 0.2], "index": 0}],
            "model": "text-embedding-ada-002",
            "usage": {"prompt_tokens": 8, "completion_tokens": 0, "total_tokens": 8}
        })
    }

    #[test]
    fn test_name() {
        assert_eq!(AzureProvider::new().name(), "azure");
    }

    #[test]
    fn test_supported_models() {
        let p = AzureProvider::new();
        let models = p.supported_models();
        assert!(models.contains(&"gpt-4"));
        assert!(models.contains(&"gpt-4o"));
        assert!(models.contains(&"gpt-35-turbo"));
    }

    #[test]
    fn test_supports_streaming() {
        assert!(AzureProvider::new().supports_streaming());
    }

    #[test]
    fn test_default() {
        assert_eq!(AzureProvider::default().name(), "azure");
    }

    #[test]
    fn test_routing_weight() {
        assert_eq!(AzureProvider::new().routing_weight(), 5);
    }

    #[test]
    fn test_with_api_base() {
        let p = AzureProvider::new().with_api_base("https://custom.openai.azure.com".into());
        assert_eq!(p.name(), "azure");
    }

    #[test]
    fn test_supports_model() {
        let p = AzureProvider::new();
        assert!(p.supports_model("gpt-4"));
        assert!(!p.supports_model("claude-3"));
    }

    #[tokio::test]
    async fn completion_network_error() {
        let p = AzureProvider::new().with_api_base("http://127.0.0.1:1".into());
        assert!(p.completion(&req("gpt-4"), None).await.is_err());
    }

    #[tokio::test]
    async fn completion_success() {
        let s = MockHttpServer::with_json(&ok_response()).await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        let r = p.completion(&req("gpt-4"), Some("k")).await.unwrap();
        assert_eq!(r.choices.len(), 1);
        assert_eq!(r.choices[0].message.content, Some("Hi!".into()));
    }

    #[tokio::test]
    async fn completion_auth_401() {
        let s = MockHttpServer::unauthorized().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gpt-4"), Some("k")).await.unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[tokio::test]
    async fn completion_auth_403() {
        let s = MockHttpServer::forbidden().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gpt-4"), Some("k")).await.unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[tokio::test]
    async fn completion_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gpt-4"), Some("k")).await.unwrap_err(),
            ProviderError::RateLimit(_)
        ));
    }

    #[tokio::test]
    async fn completion_server_error() {
        let s = MockHttpServer::error().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gpt-4"), Some("k")).await.unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn completion_bad_json() {
        let s = MockHttpServer::with_response(reqwest::StatusCode::OK, "not-json").await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(p.completion(&req("gpt-4"), None).await.is_err());
    }

    #[tokio::test]
    async fn embedding_success() {
        let s = MockHttpServer::with_json(&ok_embeddings()).await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        let r = p
            .embedding(
                &HttpEmbeddingRequest {
                    input: "hello".into(),
                    model: "text-embedding-ada-002".into(),
                    api_base: None,
                    timeout: None,
                },
                Some("k"),
            )
            .await
            .unwrap();
        assert_eq!(r.data.len(), 1);
        assert_eq!(r.data[0].embedding, vec![0.1, 0.2]);
    }

    #[tokio::test]
    async fn embedding_auth_401() {
        let s = MockHttpServer::unauthorized().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(p
            .embedding(
                &HttpEmbeddingRequest {
                    input: "t".into(),
                    model: "m".into(),
                    api_base: None,
                    timeout: None,
                },
                Some("k"),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn embedding_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(p
            .embedding(
                &HttpEmbeddingRequest {
                    input: "t".into(),
                    model: "m".into(),
                    api_base: None,
                    timeout: None,
                },
                Some("k"),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn embedding_server_error() {
        let s = MockHttpServer::error().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(p
            .embedding(
                &HttpEmbeddingRequest {
                    input: "t".into(),
                    model: "m".into(),
                    api_base: None,
                    timeout: None,
                },
                Some("k"),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_auth_error() {
        let s = MockHttpServer::unauthorized().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(p.streaming_completion(&req("gpt-4"), Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn streaming_completion_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(p.streaming_completion(&req("gpt-4"), Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn streaming_completion_server_error() {
        let s = MockHttpServer::error().await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        assert!(p.streaming_completion(&req("gpt-4"), Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn streaming_completion_success() {
        let s = MockHttpServer::start(|_| {
            hyper::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body("data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: [DONE]\n\n".to_string())
                .unwrap()
        })
        .await;
        let p = AzureProvider::new().with_api_base(s.base_url());
        let mut r = p.streaming_completion(&req("gpt-4"), Some("k")).await.unwrap();
        let chunk = r.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }

    #[tokio::test]
    async fn streaming_completion_network_error() {
        let p = AzureProvider::new().with_api_base("http://127.0.0.1:1".into());
        assert!(p.streaming_completion(&req("gpt-4"), None).await.is_err());
    }

    #[tokio::test]
    async fn default_trait_methods() {
        let p = AzureProvider::new();
        assert!(p.get_response("id", None, None, None).await.is_err());
        assert!(p.delete_response("id", None, None, None).await.is_err());
        let batch_req = HttpBatchCreateRequest {
            input_file: "f".into(),
            endpoint: "/v1".into(),
            completion_window: "24h".into(),
            metadata: None,
            api_base: None,
            timeout: None,
        };
        assert!(p.batch_create(&batch_req, None).await.is_err());
        assert!(p.batch_retrieve("id", None, None, None).await.is_err());
        assert!(p.batch_cancel("id", None, None, None).await.is_err());
        assert!(p.batch_list(None, None, None, None).await.is_err());
        assert!(p.batch_results("id", None, None, None).await.is_err());
        assert!(p.list_models(None, None, None).await.is_err());
    }
}
