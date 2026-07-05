// replicate — Replicate via reqwest (native_http, LiteLLM mode)

use super::{
    HttpBatchCreateRequest, HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest,
    HttpEmbeddingResponse, HttpResponsesRequest, ProviderError, StreamingChunk, StreamingResponse,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;

pub struct ReplicateProvider {
    client: Client,
    api_base: String,
}

impl ReplicateProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.replicate.com/v1".to_string(),
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }
}

impl Default for ReplicateProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for ReplicateProvider {
    fn name(&self) -> &str {
        "replicate"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "meta/llama-3-70b-instruct",
            "meta/llama-3-8b-instruct",
            "mistralai/mixtral-8x22b",
            "mistralai/pixtral-12b",
            "deepseek-ai/deepseek-v3",
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
        // Replicate uses a predictions API - first create a prediction, then poll
        let create_url = format!("{}/predictions", self.api_base);

        let last_msg = request
            .messages
            .last()
            .ok_or_else(|| ProviderError::InvalidResponse("No messages provided".to_string()))?;

        let create_body = serde_json::json!({
            "version": request.model,
            "input": {
                "prompt": last_msg.content,
                "max_tokens": request.max_tokens.unwrap_or(1024),
            }
        });

        let mut req_builder = self
            .client
            .post(&create_url)
            .header("Content-Type", "application/json")
            .json(&create_body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        let create_resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !create_resp.status().is_success() {
            let status = create_resp.status();
            let err_body = create_resp.text().await.unwrap_or_default();
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

        let prediction: ReplicatePrediction = create_resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        // Poll for completion
        let output = loop {
            let status_url = prediction
                .urls
                .status
                .as_ref()
                .or(prediction.urls.cancel.as_ref());
            let poll_url = status_url
                .cloned()
                .unwrap_or_else(|| prediction.urls.get.as_deref().unwrap_or("").to_string());

            let mut poll_builder = self.client.get(poll_url);
            if let Some(key) = api_key {
                poll_builder = poll_builder.header("Authorization", format!("Bearer {}", key));
            }
            let poll_resp = poll_builder
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;

            let status: ReplicateStatus = poll_resp
                .json()
                .await
                .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

            match status.status.as_str() {
                "succeeded" => break status.output,
                "failed" => {
                    return Err(ProviderError::InvalidResponse(
                        "Prediction failed".to_string(),
                    ))
                }
                "canceled" => {
                    return Err(ProviderError::InvalidResponse(
                        "Prediction canceled".to_string(),
                    ))
                }
                _ => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        };

        let output_text = output.as_str().unwrap_or("").to_string();

        Ok(HttpCompletionResponse {
            id: format!("replicate-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new("assistant", output_text),
                "stop".to_string(),
            )],
            usage: crate::shared_types::Usage::new(0, 0, 0),
            metadata: None,
        })
    }

    async fn embedding(
        &self,
        _request: &HttpEmbeddingRequest,
        _api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel(
            "Replicate does not support embeddings".to_string(),
        ))
    }

    fn routing_weight(&self) -> u32 {
        3
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        // Replicate uses a streaming predictions API
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let create_url = format!("{}/predictions", base_url);

        let last_msg = request
            .messages
            .last()
            .ok_or_else(|| ProviderError::InvalidResponse("No messages provided".to_string()))?;

        let create_body = serde_json::json!({
            "version": request.model,
            "stream": true,
            "input": {
                "prompt": last_msg.content,
                "max_tokens": request.max_tokens.unwrap_or(1024),
            }
        });

        let mut req_builder = self
            .client
            .post(&create_url)
            .header("Content-Type", "application/json")
            .json(&create_body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        let create_resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !create_resp.status().is_success() {
            let status = create_resp.status();
            let text = create_resp.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        let prediction: ReplicatePrediction = create_resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        // Get the streaming URL from prediction
        let stream_url = prediction
            .urls
            .stream
            .or(prediction.urls.get)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("No streaming URL available".to_string())
            })?;

        let mut stream_builder = self
            .client
            .get(&stream_url)
            .header("Accept", "text/event-stream");
        if let Some(key) = api_key {
            stream_builder = stream_builder.header("Authorization", format!("Bearer {}", key));
        }
        let resp = stream_builder
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
            let chunk_id = format!("replicate-{}", uuid::Uuid::new_v4());
            let created = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        // Replicate SSE format: process complete lines
                        while let Some(newline_pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = buffer.drain(..=newline_pos).collect();
                            let line_str = match std::str::from_utf8(&line) {
                                Ok(s) => s.trim(),
                                Err(_) => continue,
                            };
                            if line_str.is_empty() {
                                continue;
                            }

                            // Parse SSE event: "event: <type>" and "data: <json>"
                            if let Some(data_str) = line_str.strip_prefix("data: ") {
                                if data_str == "[DONE]" {
                                    let _ = tx
                                        .send(Ok(StreamingChunk::RawSSE(
                                            "data: [DONE]\n\n".as_bytes().to_vec(),
                                        )))
                                        .await;
                                    break;
                                }

                                // Parse Replicate streaming event
                                if let Ok(event) =
                                    serde_json::from_str::<serde_json::Value>(data_str)
                                {
                                    // Replicate sends output as strings or arrays
                                    if let Some(output) = event.get("output") {
                                        let text = if let Some(s) = output.as_str() {
                                            s.to_string()
                                        } else if let Some(arr) = output.as_array() {
                                            arr.iter()
                                                .filter_map(|v| v.as_str())
                                                .collect::<Vec<_>>()
                                                .join("")
                                        } else {
                                            continue;
                                        };

                                        if !text.is_empty() {
                                            let openai_chunk = format!(
                                                "data: {{\"id\":\"{}\",\"object\":\"chat.completion.chunk\",\"created\":{},\"model\":\"{}\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{}\"}},\"finish_reason\":null}}]}}\n\n",
                                                chunk_id,
                                                created,
                                                model,
                                                text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
                                            );
                                            if tx
                                                .send(Ok(StreamingChunk::RawSSE(
                                                    openai_chunk.into_bytes(),
                                                )))
                                                .await
                                                .is_err()
                                            {
                                                break;
                                            }
                                        }
                                    }

                                    // Check for completed status
                                    if let Some(status) =
                                        event.get("status").and_then(|s| s.as_str())
                                    {
                                        if status == "succeeded" || status == "failed" {
                                            let finish_reason = if status == "succeeded" {
                                                "stop"
                                            } else {
                                                "length"
                                            };
                                            let finish_chunk = format!(
                                                "data: {{\"id\":\"{}\",\"object\":\"chat.completion.chunk\",\"created\":{},\"model\":\"{}\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"{}\"}}]}}\n\n",
                                                chunk_id, created, model, finish_reason
                                            );
                                            let _ = tx
                                                .send(Ok(StreamingChunk::RawSSE(
                                                    finish_chunk.into_bytes(),
                                                )))
                                                .await;
                                            let _ = tx
                                                .send(Ok(StreamingChunk::RawSSE(
                                                    "data: [DONE]\n\n".as_bytes().to_vec(),
                                                )))
                                                .await;
                                            break;
                                        }
                                    }
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

    fn req_with_api_base(model: &str, api_base: &str) -> HttpCompletionRequest {
        HttpCompletionRequest {
            api_base: Some(api_base.into()),
            ..req(model)
        }
    }

    #[test]
    fn test_name() {
        assert_eq!(ReplicateProvider::new().name(), "replicate");
    }

    #[test]
    fn test_supported_models() {
        let p = ReplicateProvider::new();
        let models = p.supported_models();
        assert!(models.contains(&"meta/llama-3-70b-instruct"));
        assert!(models.contains(&"meta/llama-3-8b-instruct"));
        assert!(models.contains(&"mistralai/mixtral-8x22b"));
        assert!(models.contains(&"mistralai/pixtral-12b"));
        assert!(models.contains(&"deepseek-ai/deepseek-v3"));
        assert_eq!(models.len(), 5);
    }

    #[test]
    fn test_supports_streaming() {
        assert!(ReplicateProvider::new().supports_streaming());
    }

    #[test]
    fn test_default() {
        let p = ReplicateProvider::default();
        assert_eq!(p.name(), "replicate");
    }

    #[test]
    fn test_routing_weight() {
        assert_eq!(ReplicateProvider::new().routing_weight(), 3);
    }

    #[test]
    fn test_supports_model() {
        let p = ReplicateProvider::new();
        assert!(p.supports_model("meta/llama-3-70b-instruct"));
        assert!(!p.supports_model("gpt-4"));
    }

    #[tokio::test]
    async fn embedding_unsupported() {
        let p = ReplicateProvider::new();
        let err = p
            .embedding(
                &HttpEmbeddingRequest {
                    input: "test".into(),
                    model: "m".into(),
                    api_base: None,
                    timeout: None,
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn get_response_unsupported() {
        let p = ReplicateProvider::new();
        let err = p.get_response("id", None, None, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn delete_response_unsupported() {
        let p = ReplicateProvider::new();
        let err = p.delete_response("id", None, None, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn create_response_unsupported() {
        let p = ReplicateProvider::new();
        let req = HttpResponsesRequest {
            model: "m".into(),
            input: serde_json::json!("test"),
            instructions: None,
            temperature: None,
            max_output_tokens: None,
            top_p: None,
            stream: None,
            tools: None,
            tool_choice: None,
            store: None,
            metadata: None,
            user: None,
            api_base: None,
            timeout: None,
        };
        let err = p.create_response(&req, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn batch_create_unsupported() {
        let p = ReplicateProvider::new();
        let req = HttpBatchCreateRequest {
            input_file: "f".into(),
            endpoint: "/v1/chat/completions".into(),
            completion_window: "24h".into(),
            metadata: None,
            api_base: None,
            timeout: None,
        };
        let err = p.batch_create(&req, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn batch_retrieve_unsupported() {
        let p = ReplicateProvider::new();
        let err = p.batch_retrieve("id", None, None, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn batch_cancel_unsupported() {
        let p = ReplicateProvider::new();
        let err = p.batch_cancel("id", None, None, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn batch_list_unsupported() {
        let p = ReplicateProvider::new();
        let err = p.batch_list(None, None, None, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn batch_results_unsupported() {
        let p = ReplicateProvider::new();
        let err = p.batch_results("id", None, None, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn list_models_unsupported() {
        let p = ReplicateProvider::new();
        let err = p.list_models(None, None, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::UnsupportedModel(_)));
    }

    #[tokio::test]
    async fn completion_network_error() {
        let p = ReplicateProvider::new().with_api_base("http://127.0.0.1:1".into());
        let err = p.completion(&req("m"), None).await.unwrap_err();
        assert!(matches!(err, ProviderError::Network(_)));
    }

    #[tokio::test]
    async fn completion_success() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": "http://mock/status",
                "get": null,
                "cancel": null,
                "stream": null
            }
        });
        let status_json = serde_json::json!({
            "status": "succeeded",
            "output": "Hello from replicate"
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("status") || uri.contains("predictions") && req.method() == "GET" {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(status_json.to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let r = p.completion(&req("m"), Some("k")).await.unwrap();
        assert_eq!(r.choices.len(), 1);
        assert_eq!(
            r.choices[0].message.content,
            Some("Hello from replicate".into())
        );
    }

    #[tokio::test]
    async fn completion_no_messages() {
        let p = ReplicateProvider::new();
        let empty_req = HttpCompletionRequest {
            model: "m".into(),
            messages: vec![],
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
        let err = p.completion(&empty_req, None).await.unwrap_err();
        assert!(matches!(err, ProviderError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn completion_create_auth_error() {
        let server = MockHttpServer::unauthorized().await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::AuthError(_)));
    }

    #[tokio::test]
    async fn completion_create_rate_limit() {
        let server = MockHttpServer::rate_limited().await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::RateLimit(_)));
    }

    #[tokio::test]
    async fn completion_create_server_error() {
        let server = MockHttpServer::error().await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn completion_prediction_failed() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": "http://mock/status",
                "get": null,
                "cancel": null,
                "stream": null
            }
        });
        let status_json = serde_json::json!({
            "status": "failed",
            "output": ""
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("status") || uri.contains("predictions") && req.method() == "GET" {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(status_json.to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn completion_prediction_canceled() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": "http://mock/status",
                "get": null,
                "cancel": null,
                "stream": null
            }
        });
        let status_json = serde_json::json!({
            "status": "canceled",
            "output": ""
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("status") || uri.contains("predictions") && req.method() == "GET" {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(status_json.to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn streaming_completion_no_stream_url() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": null,
                "get": null,
                "cancel": null,
                "stream": null
            }
        });
        let server = MockHttpServer::start(move |_req| {
            hyper::Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(prediction_json.to_string())
                .unwrap()
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.streaming_completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn streaming_completion_network_error() {
        let p = ReplicateProvider::new().with_api_base("http://127.0.0.1:1".into());
        let err = p.streaming_completion(&req("m"), None).await.unwrap_err();
        assert!(matches!(err, ProviderError::Network(_)));
    }

    #[tokio::test]
    async fn streaming_completion_create_error() {
        let server = MockHttpServer::unauthorized().await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.streaming_completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::AuthError(_)));
    }

    #[tokio::test]
    async fn streaming_completion_success() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": null,
                "get": null,
                "cancel": null,
                "stream": "http://mock/stream"
            }
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("stream") {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "text/event-stream")
                    .body("data: {\"output\": \"Hello\"}\n\ndata: {\"status\": \"succeeded\"}\n\n".to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let mut r = p.streaming_completion(&req("m"), Some("k")).await.unwrap();
        let chunk = r.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }

    #[tokio::test]
    async fn streaming_completion_stream_auth_error() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": null,
                "get": null,
                "cancel": null,
                "stream": "http://mock/stream"
            }
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("stream") {
                hyper::Response::builder()
                    .status(401)
                    .body("Unauthorized".to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.streaming_completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::AuthError(_)));
    }

    #[tokio::test]
    async fn streaming_completion_stream_rate_limit() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": null,
                "get": null,
                "cancel": null,
                "stream": "http://mock/stream"
            }
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("stream") {
                hyper::Response::builder()
                    .status(429)
                    .body("Rate Limited".to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.streaming_completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::RateLimit(_)));
    }

    #[tokio::test]
    async fn streaming_completion_stream_server_error() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": null,
                "get": null,
                "cancel": null,
                "stream": "http://mock/stream"
            }
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("stream") {
                hyper::Response::builder()
                    .status(500)
                    .body("Server Error".to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let err = p.streaming_completion(&req("m"), Some("k")).await.unwrap_err();
        assert!(matches!(err, ProviderError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn streaming_output_array() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": null,
                "get": null,
                "cancel": null,
                "stream": "http://mock/stream"
            }
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("stream") {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "text/event-stream")
                    .body("data: {\"output\": [\"Hello\", \" World\"]}\n\ndata: {\"status\": \"succeeded\"}\n\n".to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let mut r = p.streaming_completion(&req("m"), Some("k")).await.unwrap();
        let chunk = r.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }

    #[tokio::test]
    async fn streaming_done_marker() {
        let prediction_json = serde_json::json!({
            "id": "pred_123",
            "urls": {
                "status": null,
                "get": null,
                "cancel": null,
                "stream": "http://mock/stream"
            }
        });
        let server = MockHttpServer::start(move |req| {
            let uri = req.uri().to_string();
            if uri.contains("stream") {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "text/event-stream")
                    .body("data: [DONE]\n\n".to_string())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(prediction_json.to_string())
                    .unwrap()
            }
        })
        .await;
        let p = ReplicateProvider::new().with_api_base(server.base_url());
        let mut r = p.streaming_completion(&req("m"), Some("k")).await.unwrap();
        let chunk = r.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ReplicatePrediction {
    id: String,
    urls: ReplicateUrls,
}

#[derive(serde::Deserialize)]
struct ReplicateUrls {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    cancel: Option<String>,
    #[serde(default)]
    get: Option<String>,
    #[serde(default)]
    stream: Option<String>,
}

#[derive(serde::Deserialize)]
struct ReplicateStatus {
    status: String,
    output: serde_json::Value,
}
