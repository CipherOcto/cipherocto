// together — Together AI via reqwest (native_http, LiteLLM mode)

use crate::native_http::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingResponse,
};
use async_trait::async_trait;
use reqwest::Client;

pub struct TogetherProvider {
    client: Client,
    api_base: String,
}

impl TogetherProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.together.xyz/v1".to_string(),
        }
    }
}

impl Default for TogetherProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for TogetherProvider {
    fn name(&self) -> &str {
        "together"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "meta-llama/Llama-3-70b-chat",
            "meta-llama/Llama-3-8b-chat",
            "mistralai/Mixtral-8x22b",
            "mistralai/Mixtral-8x7b",
            "Qwen/Qwen2-72B",
            "Qwen/Qwen2-7B",
            "deepseek-ai/DeepSeek-V3",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
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

        let data: TogetherResponse = resp
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
        let url = format!("{}/embeddings", self.api_base);

        let body = serde_json::json!({
            "input": request.input,
            "model": request.model
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

        let data: TogetherEmbeddingsResponse = resp
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
        api_key: Option<&str>,
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
        5
    }
}

#[derive(serde::Deserialize)]
struct TogetherResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<TogetherChoice>,
    usage: TogetherUsage,
}

#[derive(serde::Deserialize)]
struct TogetherChoice {
    index: u32,
    message: TogetherMessage,
    finish_reason: String,
}

#[derive(serde::Deserialize)]
struct TogetherMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct TogetherUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct TogetherEmbeddingsResponse {
    object: String,
    data: Vec<TogetherEmbedding>,
    model: String,
    usage: TogetherUsage,
}

#[derive(serde::Deserialize)]
struct TogetherEmbedding {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_http::HttpProvider;
    use crate::native_http::{HttpBatchCreateRequest, StreamingChunk};
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

    fn ok_embeddings() -> serde_json::Value {
        serde_json::json!({
            "object": "list",
            "data": [{"object": "embedding", "embedding": [0.1, 0.2], "index": 0}],
            "model": "togethercomputer/llama-3-8b",
            "usage": {"prompt_tokens": 8, "completion_tokens": 0, "total_tokens": 8}
        })
    }

    #[test]
    fn test_name() {
        assert_eq!(TogetherProvider::new().name(), "together");
    }

    #[test]
    fn test_supported_models() {
        let p = TogetherProvider::new();
        let models = p.supported_models();
        assert!(models.contains(&"meta-llama/Llama-3-70b-chat"));
        assert!(models.contains(&"deepseek-ai/DeepSeek-V3"));
    }

    #[test]
    fn test_supports_streaming() {
        assert!(TogetherProvider::new().supports_streaming());
    }

    #[test]
    fn test_default() {
        assert_eq!(TogetherProvider::default().name(), "together");
    }

    #[test]
    fn test_routing_weight() {
        assert_eq!(TogetherProvider::new().routing_weight(), 5);
    }

    #[test]
    fn test_supports_model() {
        let p = TogetherProvider::new();
        assert!(p.supports_model("meta-llama/Llama-3-70b-chat"));
        assert!(!p.supports_model("gpt-4"));
    }

    #[tokio::test]
    async fn completion_network_error() {
        let p = TogetherProvider::new();
        let mut r = req("m");
        r.api_base = Some("http://127.0.0.1:1".into());
        assert!(p.completion(&r, None).await.is_err());
    }

    #[tokio::test]
    async fn completion_auth_401() {
        let s = MockHttpServer::unauthorized().await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(matches!(
            TogetherProvider::new()
                .completion(&r, Some("k"))
                .await
                .unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[tokio::test]
    async fn embedding_success() {
        let s = MockHttpServer::with_json(&ok_embeddings()).await;
        let p = TogetherProvider::new();
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        let _ = p
            .embedding(
                &HttpEmbeddingRequest {
                    input: "hello".into(),
                    model: "m".into(),
                    api_base: Some(s.base_url()),
                    timeout: None,
                },
                Some("k"),
            )
            .await;
    }

    #[tokio::test]
    async fn streaming_completion_auth_error() {
        let s = MockHttpServer::unauthorized().await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(TogetherProvider::new()
            .streaming_completion(&r, Some("k"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(TogetherProvider::new()
            .streaming_completion(&r, Some("k"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_server_error() {
        let s = MockHttpServer::error().await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(TogetherProvider::new()
            .streaming_completion(&r, Some("k"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_success() {
        let s = MockHttpServer::start(|_| {
            hyper::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: [DONE]\n\n"
                        .to_string(),
                )
                .unwrap()
        })
        .await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        let mut resp = TogetherProvider::new()
            .streaming_completion(&r, Some("k"))
            .await
            .unwrap();
        let chunk = resp.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }

    #[tokio::test]
    async fn default_trait_methods() {
        let p = TogetherProvider::new();
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
