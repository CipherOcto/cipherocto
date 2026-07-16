// gemini — Google Gemini via reqwest (native_http, LiteLLM mode)

use super::{
    HttpBatchCreateRequest, HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest,
    HttpEmbeddingResponse, ProviderError, StreamingChunk, StreamingResponse,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;

pub struct GeminiProvider {
    client: Client,
    api_base: String,
}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-1.5-pro",
            "gemini-1.5-flash",
            "gemini-1.5-flash-8b",
        ]
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        // Gemini uses generate_content endpoint, not chat completions
        let url = match api_key {
            Some(key) => format!(
                "{}/models/{}:generateContent?key={}",
                self.api_base, request.model, key
            ),
            None => format!("{}/models/{}:generateContent", self.api_base, request.model),
        };

        // Build contents for Gemini - combine messages into a single text prompt
        let prompt = request
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n");

        let body = serde_json::json!({
            "contents": [{
                "parts": [{ "text": prompt }],
                "role": "user"
            }],
            "generationConfig": {
                "temperature": request.temperature.unwrap_or(0.9),
                "maxOutputTokens": request.max_tokens.unwrap_or(2048),
                "topP": request.top_p.unwrap_or(0.95),
            }
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

        let data: GeminiResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let text = data
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .and_then(|p| p.text.as_ref())
            .unwrap_or(&String::new())
            .clone();

        Ok(HttpCompletionResponse {
            id: format!("gemini-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new("model", text),
                data.candidates
                    .first()
                    .and_then(|c| c.finish_reason.as_ref())
                    .unwrap_or(&"stop".to_string())
                    .clone(),
            )],
            usage: crate::shared_types::Usage::new(
                data.usage_metadata.prompt_token_count.unwrap_or(0),
                data.usage_metadata.candidates_token_count.unwrap_or(0),
                data.usage_metadata.total_token_count.unwrap_or(0),
            ),
            metadata: None,
        })
    }

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        let url = match api_key {
            Some(key) => format!(
                "{}/models/{}:embedContent?key={}",
                self.api_base, request.model, key
            ),
            None => format!("{}/models/{}:embedContent", self.api_base, request.model),
        };

        let body = serde_json::json!({
            "content": { "parts": [{ "text": request.input }] }
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

        let data: GeminiEmbeddingsResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpEmbeddingResponse {
            object: "list".to_string(),
            data: vec![crate::shared_types::Embedding {
                object: "embedding".to_string(),
                embedding: data.embedding.values,
                index: 0,
            }],
            model: request.model.clone(),
            usage: crate::shared_types::Usage::new(0, 0, 0),
        })
    }

    fn routing_weight(&self) -> u32 {
        6
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        // Gemini uses streamGenerateContent endpoint for streaming
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = match api_key {
            Some(key) => format!(
                "{}/models/{}:streamGenerateContent?key={}",
                base_url, request.model, key
            ),
            None => format!(
                "{}/models/{}:streamGenerateContent",
                base_url, request.model
            ),
        };

        // Build contents for Gemini - combine messages into a single text prompt
        let prompt = request
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n");

        let body = serde_json::json!({
            "contents": [{
                "parts": [{ "text": prompt }],
                "role": "user"
            }],
            "generationConfig": {
                "temperature": request.temperature.unwrap_or(0.9),
                "maxOutputTokens": request.max_tokens.unwrap_or(2048),
                "topP": request.top_p.unwrap_or(0.95),
            }
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

        let (tx, rx) = mpsc::channel(100);
        let model = request.model.clone();

        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            let chunk_id = format!("gemini-{}", uuid::Uuid::new_v4());
            let created = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        // Gemini streams newline-delimited JSON objects
                        // Process complete lines from buffer
                        while let Some(newline_pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = buffer.drain(..=newline_pos).collect();
                            let line_str = match std::str::from_utf8(&line) {
                                Ok(s) => s.trim(),
                                Err(_) => continue,
                            };
                            if line_str.is_empty() {
                                continue;
                            }
                            // Skip array brackets at start/end of stream
                            if line_str == "[" || line_str == "]" {
                                continue;
                            }
                            // Remove trailing comma if present
                            let json_str = line_str.trim_end_matches(',');

                            // Parse Gemini response chunk
                            if let Ok(chunk) = serde_json::from_str::<GeminiStreamChunk>(json_str) {
                                if let Some(text) = chunk
                                    .candidates
                                    .first()
                                    .and_then(|c| c.content.parts.first())
                                    .and_then(|p| p.text.as_ref())
                                {
                                    let openai_chunk = format!(
                                        "data: {{\"id\":\"{}\",\"object\":\"chat.completion.chunk\",\"created\":{},\"model\":\"{}\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{}\"}},\"finish_reason\":null}}]}}\n\n",
                                        chunk_id,
                                        created,
                                        model,
                                        text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
                                    );
                                    if tx
                                        .send(Ok(StreamingChunk::RawSSE(openai_chunk.into_bytes())))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }

                                // Check for finish reason
                                let finish_reason = chunk
                                    .candidates
                                    .first()
                                    .and_then(|c| c.finish_reason.as_ref());
                                if let Some(reason) = finish_reason {
                                    let finish_chunk = format!(
                                        "data: {{\"id\":\"{}\",\"object\":\"chat.completion.chunk\",\"created\":{},\"model\":\"{}\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"{}\"}}]}}\n\n",
                                        chunk_id, created, model, reason
                                    );
                                    let _ = tx
                                        .send(Ok(StreamingChunk::RawSSE(finish_chunk.into_bytes())))
                                        .await;
                                    // Send DONE marker
                                    let _ = tx
                                        .send(Ok(StreamingChunk::RawSSE(
                                            "data: [DONE]\n\n".as_bytes().to_vec(),
                                        )))
                                        .await;
                                }
                            }
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

#[derive(serde::Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: GeminiUsage,
}

#[derive(serde::Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(serde::Deserialize)]
struct GeminiPart {
    #[serde(default)]
    text: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct GeminiUsage {
    #[serde(default)]
    prompt_token_count: Option<u32>,
    #[serde(default)]
    candidates_token_count: Option<u32>,
    #[serde(default)]
    total_token_count: Option<u32>,
}

#[derive(serde::Deserialize)]
struct GeminiEmbeddingsResponse {
    embedding: GeminiEmbeddingValues,
}

#[derive(serde::Deserialize)]
struct GeminiEmbeddingValues {
    values: Vec<f32>,
}

/// Gemini streaming response chunk
#[derive(serde::Deserialize)]
struct GeminiStreamChunk {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
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
            "candidates": [{
                "content": {"parts": [{"text": "Hello from Gemini!"}]},
                "finish_reason": "STOP"
            }],
            "usage_metadata": {
                "prompt_token_count": 10,
                "candidates_token_count": 5,
                "total_token_count": 15
            }
        })
    }

    #[test]
    fn test_name() {
        assert_eq!(GeminiProvider::new().name(), "gemini");
    }

    #[test]
    fn test_supported_models() {
        let p = GeminiProvider::new();
        let models = p.supported_models();
        assert!(models.contains(&"gemini-2.5-flash"));
        assert!(models.contains(&"gemini-2.5-pro"));
        assert!(models.contains(&"gemini-1.5-pro"));
        assert!(models.contains(&"gemini-1.5-flash"));
        assert!(models.contains(&"gemini-1.5-flash-8b"));
    }

    #[test]
    fn test_supports_streaming() {
        assert!(GeminiProvider::new().supports_streaming());
    }

    #[test]
    fn test_default() {
        assert_eq!(GeminiProvider::default().name(), "gemini");
    }

    #[test]
    fn test_routing_weight() {
        assert_eq!(GeminiProvider::new().routing_weight(), 6);
    }

    #[test]
    fn test_with_api_base() {
        let p = GeminiProvider::new().with_api_base("https://custom.gemini.com".into());
        assert_eq!(p.name(), "gemini");
    }

    #[tokio::test]
    async fn completion_network_error() {
        let p = GeminiProvider::new().with_api_base("http://127.0.0.1:1".into());
        let err = p
            .completion(&req("gemini-2.5-flash"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Network(_)));
    }

    #[tokio::test]
    async fn completion_success() {
        let s = MockHttpServer::with_json(&ok_response()).await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        let r = p
            .completion(&req("gemini-2.5-flash"), Some("k"))
            .await
            .unwrap();
        assert_eq!(r.choices.len(), 1);
        assert_eq!(
            r.choices[0].message.content,
            Some("Hello from Gemini!".into())
        );
        assert_eq!(r.usage.prompt_tokens, 10);
        assert_eq!(r.usage.completion_tokens, 5);
    }

    #[tokio::test]
    async fn completion_auth_401() {
        let s = MockHttpServer::unauthorized().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gemini-2.5-flash"), Some("k"))
                .await
                .unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[tokio::test]
    async fn completion_auth_403() {
        let s = MockHttpServer::forbidden().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gemini-2.5-flash"), Some("k"))
                .await
                .unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[tokio::test]
    async fn completion_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gemini-2.5-flash"), Some("k"))
                .await
                .unwrap_err(),
            ProviderError::RateLimit(_)
        ));
    }

    #[tokio::test]
    async fn completion_server_error() {
        let s = MockHttpServer::error().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gemini-2.5-flash"), Some("k"))
                .await
                .unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn completion_bad_json() {
        let s = MockHttpServer::with_response(reqwest::StatusCode::OK, "not-json").await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        assert!(matches!(
            p.completion(&req("gemini-2.5-flash"), None)
                .await
                .unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn completion_no_finish_reason() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": {"parts": [{"text": "Hi"}]}
            }],
            "usage_metadata": {
                "prompt_token_count": 10,
                "candidates_token_count": 1,
                "total_token_count": 11
            }
        });
        let s = MockHttpServer::with_json(&resp).await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        let r = p
            .completion(&req("gemini-2.5-flash"), Some("k"))
            .await
            .unwrap();
        assert_eq!(r.choices[0].finish_reason, "stop");
    }

    #[tokio::test]
    async fn completion_empty_candidates() {
        let resp = serde_json::json!({
            "candidates": [],
            "usage_metadata": {"total_token_count": 0}
        });
        let s = MockHttpServer::with_json(&resp).await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        let r = p
            .completion(&req("gemini-2.5-flash"), Some("k"))
            .await
            .unwrap();
        assert_eq!(r.choices[0].message.content, Some("".into()));
    }

    #[tokio::test]
    async fn completion_no_api_key() {
        let s = MockHttpServer::with_json(&ok_response()).await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        let r = p.completion(&req("gemini-2.5-flash"), None).await.unwrap();
        assert_eq!(r.choices.len(), 1);
    }

    #[tokio::test]
    async fn embedding_success() {
        let resp = serde_json::json!({
            "embedding": {"values": [0.1, 0.2, 0.3]}
        });
        let s = MockHttpServer::with_json(&resp).await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        let r = p
            .embedding(
                &HttpEmbeddingRequest {
                    input: "hello".into(),
                    model: "text-embedding".into(),
                    api_base: None,
                    timeout: None,
                },
                Some("k"),
            )
            .await
            .unwrap();
        assert_eq!(r.data.len(), 1);
        assert_eq!(r.data[0].embedding, vec![0.1, 0.2, 0.3]);
    }

    #[tokio::test]
    async fn embedding_auth_error() {
        let s = MockHttpServer::unauthorized().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
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
        let p = GeminiProvider::new().with_api_base(s.base_url());
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

    #[tokio::test]
    async fn embedding_server_error() {
        let s = MockHttpServer::error().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
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

    #[tokio::test]
    async fn embedding_network_error() {
        let p = GeminiProvider::new().with_api_base("http://127.0.0.1:1".into());
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

    #[tokio::test]
    async fn streaming_completion_network_error() {
        let p = GeminiProvider::new().with_api_base("http://127.0.0.1:1".into());
        assert!(p
            .streaming_completion(&req("gemini-2.5-flash"), None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_auth_error() {
        let s = MockHttpServer::unauthorized().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        assert!(p
            .streaming_completion(&req("gemini-2.5-flash"), Some("k"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        assert!(p
            .streaming_completion(&req("gemini-2.5-flash"), Some("k"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_server_error() {
        let s = MockHttpServer::error().await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        assert!(p
            .streaming_completion(&req("gemini-2.5-flash"), Some("k"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn streaming_completion_success() {
        let s = MockHttpServer::start(|_| {
            hyper::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body("[\n{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hi\"}]},\"finish_reason\":\"STOP\"}]}\n]\n".to_string())
                .unwrap()
        })
        .await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        let mut r = p
            .streaming_completion(&req("gemini-2.5-flash"), Some("k"))
            .await
            .unwrap();
        let chunk = r.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }

    #[tokio::test]
    async fn streaming_skip_brackets() {
        let s = MockHttpServer::start(|_| {
            hyper::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body("[\n]\n".to_string())
                .unwrap()
        })
        .await;
        let p = GeminiProvider::new().with_api_base(s.base_url());
        let mut r = p
            .streaming_completion(&req("gemini-2.5-flash"), Some("k"))
            .await
            .unwrap();
        let chunk = r.receiver.recv().await;
        assert!(chunk.is_none());
    }

    #[test]
    fn test_supports_model() {
        let p = GeminiProvider::new();
        assert!(p.supports_model("gemini-2.5-flash"));
        assert!(!p.supports_model("gpt-4o"));
    }

    #[tokio::test]
    async fn get_response_unsupported() {
        let p = GeminiProvider::new();
        assert!(p.get_response("id", None, None, None).await.is_err());
    }

    #[tokio::test]
    async fn delete_response_unsupported() {
        let p = GeminiProvider::new();
        assert!(p.delete_response("id", None, None, None).await.is_err());
    }

    #[tokio::test]
    async fn batch_create_unsupported() {
        let p = GeminiProvider::new();
        let req = HttpBatchCreateRequest {
            input_file: "f".into(),
            endpoint: "/v1".into(),
            completion_window: "24h".into(),
            metadata: None,
            api_base: None,
            timeout: None,
        };
        assert!(p.batch_create(&req, None).await.is_err());
    }

    #[tokio::test]
    async fn list_models_unsupported() {
        let p = GeminiProvider::new();
        assert!(p.list_models(None, None, None).await.is_err());
    }
}
