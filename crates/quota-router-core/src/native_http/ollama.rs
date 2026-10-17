// Clippy `[disallowed-methods]` allowlist: this module is a
// legitimate provider-egress adapter. It talks to the model
// provider's REST API and routes the Authorization header through
// `egress::key_swap::attach_bearer` so the cipherocto-internal key
// is swapped for the provider's key before the request leaves.
// Capability tokens never reach the provider (see `egress::strip_capability`).
#![allow(clippy::disallowed_methods)]

// ollama — Ollama via reqwest (native_http, LiteLLM mode)

use crate::native_http::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingResponse,
};
use async_trait::async_trait;
use reqwest::Client;

pub struct OllamaProvider {
    client: Client,
    api_base: String,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
        }
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "llama3",
            "llama3.1",
            "llama3.2",
            "mistral",
            "mixtral",
            "phi3",
            "qwen2",
            "codellama",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        _api_key: Option<&str>,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let api_base = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/api/chat", api_base);

        let messages: Vec<_> = request
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": false
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["options"]["num_predict"] = serde_json::json!(max_tokens);
        }

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
            .header("Content-Type", "application/json")
            .json(&body)
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

        let data: OllamaResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpCompletionResponse {
            id: format!("ollama-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new("assistant", data.message.content),
                "stop".to_string(),
            )],
            usage: crate::shared_types::Usage::new(0, 0, 0),
            metadata: None,
        })
    }

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        _api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        let api_base = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/api/embeddings", api_base);

        let body = serde_json::json!({
            "model": request.model,
            "prompt": request.input
        });

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
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

        let data: OllamaEmbeddingsResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpEmbeddingResponse {
            object: "list".to_string(),
            data: vec![crate::shared_types::Embedding {
                object: "embedding".to_string(),
                embedding: data.embedding,
                index: 0,
            }],
            model: request.model.clone(),
            usage: crate::shared_types::Usage::new(0, 0, 0),
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
        3 // Lower priority for local ollama
    }
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct OllamaResponse {
    model: String,
    message: OllamaMessage,
    #[allow(dead_code)]
    done: bool,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct OllamaEmbeddingsResponse {
    embedding: Vec<f32>,
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

    #[test]
    fn test_name() {
        assert_eq!(OllamaProvider::new().name(), "ollama");
    }

    #[test]
    fn test_supported_models() {
        let p = OllamaProvider::new();
        let models = p.supported_models();
        assert!(models.contains(&"llama3"));
        assert!(models.contains(&"mistral"));
        assert!(models.contains(&"codellama"));
    }

    #[test]
    fn test_supports_streaming() {
        assert!(OllamaProvider::new().supports_streaming());
    }

    #[test]
    fn test_default() {
        assert_eq!(OllamaProvider::default().name(), "ollama");
    }

    #[test]
    fn test_routing_weight() {
        assert_eq!(OllamaProvider::new().routing_weight(), 3);
    }

    #[test]
    fn test_supports_model() {
        let p = OllamaProvider::new();
        assert!(p.supports_model("llama3"));
        assert!(!p.supports_model("gpt-4"));
    }

    #[tokio::test]
    async fn completion_network_error() {
        let p = OllamaProvider::new();
        let mut r = req("m");
        r.api_base = Some("http://127.0.0.1:1".into());
        assert!(p.completion(&r, None).await.is_err());
    }

    #[tokio::test]
    async fn completion_server_error() {
        let s = MockHttpServer::error().await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(matches!(
            OllamaProvider::new()
                .completion(&r, Some("k"))
                .await
                .unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn completion_bad_json() {
        let s = MockHttpServer::with_response(reqwest::StatusCode::OK, "not-json").await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(OllamaProvider::new().completion(&r, None).await.is_err());
    }

    #[tokio::test]
    async fn embedding_auth_error() {
        let s = MockHttpServer::unauthorized().await;
        assert!(OllamaProvider::new()
            .embedding(
                &HttpEmbeddingRequest {
                    input: "t".into(),
                    model: "m".into(),
                    api_base: Some(s.base_url()),
                    timeout: None,
                },
                None,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn embedding_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        assert!(OllamaProvider::new()
            .embedding(
                &HttpEmbeddingRequest {
                    input: "t".into(),
                    model: "m".into(),
                    api_base: Some(s.base_url()),
                    timeout: None,
                },
                None,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn embedding_server_error() {
        let s = MockHttpServer::error().await;
        assert!(OllamaProvider::new()
            .embedding(
                &HttpEmbeddingRequest {
                    input: "t".into(),
                    model: "m".into(),
                    api_base: Some(s.base_url()),
                    timeout: None,
                },
                None,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_auth_error() {
        let s = MockHttpServer::unauthorized().await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(OllamaProvider::new()
            .streaming_completion(&r, Some("k"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(OllamaProvider::new()
            .streaming_completion(&r, Some("k"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_server_error() {
        let s = MockHttpServer::error().await;
        let mut r = req("m");
        r.api_base = Some(s.base_url());
        assert!(OllamaProvider::new()
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
        let mut resp = OllamaProvider::new()
            .streaming_completion(&r, Some("k"))
            .await
            .unwrap();
        let chunk = resp.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }

    #[tokio::test]
    async fn default_trait_methods() {
        let p = OllamaProvider::new();
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
