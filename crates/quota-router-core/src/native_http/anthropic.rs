// anthropic — Anthropic via reqwest (native_http, LiteLLM mode)
//
// Per RFC-0917 lines 3185-3190: Anthropic SSE must be converted to OpenAI SSE format.

use super::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingChunk, StreamingResponse,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;

pub struct AnthropicProvider {
    client: Client,
    api_base: String,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.anthropic.com/v1".to_string(),
        }
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "claude-3-5-sonnet-latest",
            "claude-3-5-sonnet-20241022",
            "claude-3-opus-latest",
            "claude-3-opus-20240229",
            "claude-3-sonnet-latest",
            "claude-3-sonnet-20240229",
            "claude-3-haiku-latest",
            "claude-3-haiku-20240307",
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
        let api_base = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/messages", api_base);

        // Convert messages to Anthropic format
        let system = request
            .messages
            .iter()
            .filter(|m| m.role == "system")
            .filter_map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let messages: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                    "tool_calls": m.tool_calls,
                    "tool_call_id": m.tool_call_id,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        } else {
            // Anthropic requires max_tokens
            body["max_tokens"] = serde_json::json!(4096);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &request.stop {
            body["stop_sequences"] = serde_json::json!(stop);
        }
        if !system.is_empty() {
            body["system"] = serde_json::json!(system);
        }

        // Function calling fields (RFC-0939)
        if let Some(tools) = &request.tools {
            // Convert OpenAI tool format to Anthropic format
            let anthropic_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.function.name,
                        "description": t.function.description,
                        "input_schema": t.function.parameters
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(anthropic_tools);
        }
        if let Some(tool_choice) = &request.tool_choice {
            body["tool_choice"] = serde_json::to_value(tool_choice).unwrap_or_default();
        }

        let mut req_builder = self
            .client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("x-api-key", key);
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

        let data: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpCompletionResponse {
            id: format!("msg_{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new(
                    "assistant",
                    data.content
                        .first()
                        .and_then(|c| c.text.as_ref().or(c.thinking.as_ref()))
                        .unwrap_or(&String::new())
                        .clone(),
                ),
                data.stop_reason.unwrap_or_else(|| "stop".to_string()),
            )],
            usage: crate::shared_types::Usage::new(
                data.usage.input_tokens,
                data.usage.output_tokens,
                data.usage.input_tokens + data.usage.output_tokens,
            ),
            metadata: None,
        })
    }

    async fn embedding(
        &self,
        _request: &HttpEmbeddingRequest,
        _api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel(
            "Anthropic does not support embeddings".to_string(),
        ))
    }

    fn routing_weight(&self) -> u32 {
        8
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        let api_base = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/messages", api_base);

        let system = request
            .messages
            .iter()
            .filter(|m| m.role == "system")
            .filter_map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let messages: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                    "tool_calls": m.tool_calls,
                    "tool_call_id": m.tool_call_id,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": true
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        } else {
            body["max_tokens"] = serde_json::json!(4096);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &request.stop {
            body["stop_sequences"] = serde_json::json!(stop);
        }
        if !system.is_empty() {
            body["system"] = serde_json::json!(system);
        }

        let mut req_builder = self
            .client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("x-api-key", key);
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

        let (tx, rx) = mpsc::channel(100);
        let model = request.model.clone();

        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            let mut chunk_id = String::new();
            let created = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        // Parse Anthropic SSE and convert to OpenAI SSE
                        if let Some(event) = AnthropicEvent::parse(&bytes) {
                            if let Some(openai_sse) =
                                event.to_openai_sse(&chunk_id, &model, created)
                            {
                                if tx
                                    .send(Ok(StreamingChunk::RawSSE(openai_sse.into_bytes())))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            // Capture message ID from message_start
                            if let AnthropicEvent::MessageStart { id, .. } = &event {
                                chunk_id = id.clone();
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
#[allow(dead_code)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    type_: String,
    #[allow(dead_code)]
    role: String,
    content: Vec<AnthropicContentBlock>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
    #[allow(dead_code)]
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(serde::Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    block_type: String,
    text: Option<String>,
    /// MiniMax extended thinking blocks use "thinking" instead of "text"
    thinking: Option<String>,
}

#[derive(serde::Deserialize)]
struct AnthropicUsage {
    #[allow(dead_code)]
    input_tokens: u32,
    #[allow(dead_code)]
    output_tokens: u32,
}

/// Anthropic SSE event types per RFC-0917 lines 3185-3190
#[derive(Debug, Clone)]
pub enum AnthropicEvent {
    MessageStart { id: String, model: String },
    ContentBlockStart { index: u32 },
    ContentBlockDelta { index: u32, text: String },
    ContentBlockStop { index: u32 },
    MessageDelta { tokens: u32, stop_reason: String },
    MessageStop,
}

impl AnthropicEvent {
    /// Parse Anthropic SSE data line into event
    pub fn parse(data: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(data).ok()?;
        let s = s.strip_prefix("data: ")?;
        let json: serde_json::Value = serde_json::from_str(s).ok()?;

        let event_type = json.get("type")?.as_str()?;

        match event_type {
            "message_start" => {
                let msg = json.get("message")?;
                Some(AnthropicEvent::MessageStart {
                    id: msg.get("id")?.as_str()?.to_string(),
                    model: msg.get("model")?.as_str()?.to_string(),
                })
            }
            "content_block_start" => Some(AnthropicEvent::ContentBlockStart {
                index: json.get("index")?.as_u64()? as u32,
            }),
            "content_block_delta" => {
                let delta = json.get("delta")?;
                Some(AnthropicEvent::ContentBlockDelta {
                    index: json.get("index")?.as_u64()? as u32,
                    text: delta.get("text")?.as_str()?.to_string(),
                })
            }
            "content_block_stop" => Some(AnthropicEvent::ContentBlockStop {
                index: json.get("index")?.as_u64()? as u32,
            }),
            "message_delta" => {
                let delta = json.get("delta")?;
                Some(AnthropicEvent::MessageDelta {
                    tokens: delta.get("tokens")?.as_u64()? as u32,
                    stop_reason: delta.get("stop_reason")?.as_str()?.to_string(),
                })
            }
            "message_stop" => Some(AnthropicEvent::MessageStop),
            _ => None,
        }
    }

    /// Convert Anthropic event to OpenAI SSE format per RFC-0917 lines 3185-3190
    pub fn to_openai_sse(&self, chunk_id: &str, model: &str, created: u64) -> Option<String> {
        match self {
            AnthropicEvent::ContentBlockDelta { index: _, text } => {
                Some(format!(
                    "data: {{\"id\":\"{}\",\"object\":\"chat.completion.chunk\",\"created\":{},\"model\":\"{}\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{}\"}},\"finish_reason\":null}}]}}\n\n",
                    chunk_id, created, model, text.replace("\"", "\\\"").replace("\n", "\\n")
                ))
            }
            AnthropicEvent::MessageDelta { tokens: _, stop_reason } => {
                Some(format!(
                    "data: {{\"id\":\"{}\",\"object\":\"chat.completion.chunk\",\"created\":{},\"model\":\"{}\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"{}\"}}]}}\n\n",
                    chunk_id, created, model, stop_reason
                ))
            }
            AnthropicEvent::MessageStop => {
                Some("data: [DONE]\n\n".to_string())
            }
            _ => None,
        }
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

    fn req_with_api(model: &str, api_base: &str) -> HttpCompletionRequest {
        HttpCompletionRequest {
            api_base: Some(api_base.into()),
            ..req(model)
        }
    }

    fn ok_response() -> serde_json::Value {
        serde_json::json!({
            "id": "msg_abc123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello!"}],
            "stop_reason": "stop",
            "stop_sequence": null,
            "usage": {"input_tokens": 10, "output_tokens": 5}
        })
    }

    #[test]
    fn test_name() {
        assert_eq!(AnthropicProvider::new().name(), "anthropic");
    }

    #[test]
    fn test_supported_models() {
        let p = AnthropicProvider::new();
        let models = p.supported_models();
        assert!(models.contains(&"claude-3-5-sonnet-latest"));
        assert!(models.contains(&"claude-3-opus-latest"));
        assert!(models.contains(&"claude-3-haiku-latest"));
    }

    #[test]
    fn test_supports_streaming() {
        assert!(AnthropicProvider::new().supports_streaming());
    }

    #[test]
    fn test_default() {
        assert_eq!(AnthropicProvider::default().name(), "anthropic");
    }

    #[test]
    fn test_routing_weight() {
        assert_eq!(AnthropicProvider::new().routing_weight(), 8);
    }

    #[tokio::test]
    async fn embedding_unsupported() {
        let p = AnthropicProvider::new();
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
    async fn completion_network_error() {
        let p = AnthropicProvider::new();
        let err = p.completion(&req_with_api("claude-3", "http://127.0.0.1:1"), None).await.unwrap_err();
        assert!(matches!(err, ProviderError::Network(_)));
    }

    #[tokio::test]
    async fn completion_success() {
        let s = MockHttpServer::with_json(&ok_response()).await;
        let p = AnthropicProvider::new();
        let r = p.completion(&req_with_api("claude-3", &s.base_url()), Some("k")).await.unwrap();
        assert_eq!(r.choices.len(), 1);
        assert_eq!(r.choices[0].message.content, Some("Hello!".into()));
        assert_eq!(r.usage.prompt_tokens, 10);
        assert_eq!(r.usage.completion_tokens, 5);
    }

    #[tokio::test]
    async fn completion_auth_401() {
        let s = MockHttpServer::unauthorized().await;
        let p = AnthropicProvider::new();
        assert!(matches!(
            p.completion(&req_with_api("claude-3", &s.base_url()), Some("k"))
                .await
                .unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[tokio::test]
    async fn completion_auth_403() {
        let s = MockHttpServer::forbidden().await;
        let p = AnthropicProvider::new();
        assert!(matches!(
            p.completion(&req_with_api("claude-3", &s.base_url()), Some("k"))
                .await
                .unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[tokio::test]
    async fn completion_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let p = AnthropicProvider::new();
        assert!(matches!(
            p.completion(&req_with_api("claude-3", &s.base_url()), Some("k"))
                .await
                .unwrap_err(),
            ProviderError::RateLimit(_)
        ));
    }

    #[tokio::test]
    async fn completion_server_error() {
        let s = MockHttpServer::error().await;
        let p = AnthropicProvider::new();
        assert!(matches!(
            p.completion(&req_with_api("claude-3", &s.base_url()), Some("k"))
                .await
                .unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn completion_bad_json() {
        let s = MockHttpServer::with_response(reqwest::StatusCode::OK, "not-json").await;
        let p = AnthropicProvider::new();
        assert!(matches!(
            p.completion(&req_with_api("claude-3", &s.base_url()), None).await.unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn completion_with_system_message() {
        let s = MockHttpServer::with_json(&ok_response()).await;
        let p = AnthropicProvider::new();
        let mut r = req_with_api("claude-3", &s.base_url());
        r.messages.insert(
            0,
            msg("system", "You are a helpful assistant"),
        );
        let result = p.completion(&r, Some("k")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn completion_with_temperature() {
        let s = MockHttpServer::with_json(&ok_response()).await;
        let p = AnthropicProvider::new();
        let mut r = req_with_api("claude-3", &s.base_url());
        r.temperature = Some(0.5);
        r.max_tokens = Some(100);
        r.top_p = Some(0.9);
        r.stop = Some(vec!["END".into()]);
        let result = p.completion(&r, Some("k")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn completion_thinking_content() {
        let thinking_resp = serde_json::json!({
            "id": "msg_abc123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "thinking", "thinking": "Let me think..."}],
            "stop_reason": "stop",
            "stop_sequence": null,
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });
        let s = MockHttpServer::with_json(&thinking_resp).await;
        let p = AnthropicProvider::new();
        let r = p.completion(&req_with_api("claude-3", &s.base_url()), Some("k")).await.unwrap();
        assert_eq!(r.choices[0].message.content, Some("Let me think...".into()));
    }

    #[tokio::test]
    async fn completion_empty_content() {
        let empty_resp = serde_json::json!({
            "id": "msg_abc123",
            "type": "message",
            "role": "assistant",
            "content": [],
            "stop_reason": "stop",
            "stop_sequence": null,
            "usage": {"input_tokens": 10, "output_tokens": 0}
        });
        let s = MockHttpServer::with_json(&empty_resp).await;
        let p = AnthropicProvider::new();
        let r = p.completion(&req_with_api("claude-3", &s.base_url()), Some("k")).await.unwrap();
        assert_eq!(r.choices[0].message.content, Some("".into()));
    }

    #[tokio::test]
    async fn completion_no_stop_reason() {
        let resp = serde_json::json!({
            "id": "msg_abc123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hi"}],
            "stop_sequence": null,
            "usage": {"input_tokens": 10, "output_tokens": 1}
        });
        let s = MockHttpServer::with_json(&resp).await;
        let p = AnthropicProvider::new();
        let r = p.completion(&req_with_api("claude-3", &s.base_url()), Some("k")).await.unwrap();
        assert_eq!(r.choices[0].finish_reason, "stop");
    }

    #[tokio::test]
    async fn streaming_completion_network_error() {
        let p = AnthropicProvider::new();
        let err = p.streaming_completion(&req_with_api("claude-3", "http://127.0.0.1:1"), None).await.unwrap_err();
        assert!(matches!(err, ProviderError::Network(_)));
    }

    #[tokio::test]
    async fn streaming_completion_auth_error() {
        let s = MockHttpServer::unauthorized().await;
        let p = AnthropicProvider::new();
        assert!(p.streaming_completion(&req_with_api("claude-3", &s.base_url()), Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn streaming_completion_rate_limit() {
        let s = MockHttpServer::rate_limited().await;
        let p = AnthropicProvider::new();
        assert!(p.streaming_completion(&req_with_api("claude-3", &s.base_url()), Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn streaming_completion_server_error() {
        let s = MockHttpServer::error().await;
        let p = AnthropicProvider::new();
        assert!(p.streaming_completion(&req_with_api("claude-3", &s.base_url()), Some("k")).await.is_err());
    }

    #[tokio::test]
    async fn streaming_completion_success() {
        let s = MockHttpServer::start(|_| {
            hyper::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body(
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"text\":\"Hi\"}}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"tokens\":5,\"stop_reason\":\"stop\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n"
                        .to_string(),
                )
                .unwrap()
        })
        .await;
        let p = AnthropicProvider::new();
        let mut r = p
            .streaming_completion(&req_with_api("claude-3", &s.base_url()), Some("k"))
            .await
            .unwrap();
        let chunk = r.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }

    #[tokio::test]
    async fn streaming_with_system_and_temperature() {
        let s = MockHttpServer::start(|_| {
            hyper::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body(
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"text\":\"Hi\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n"
                        .to_string(),
                )
                .unwrap()
        })
        .await;
        let p = AnthropicProvider::new();
        let mut r = req_with_api("claude-3", &s.base_url());
        r.messages.insert(0, msg("system", "Be brief"));
        r.temperature = Some(0.5);
        r.max_tokens = Some(100);
        let mut resp = p.streaming_completion(&r, Some("k")).await.unwrap();
        let chunk = resp.receiver.recv().await.unwrap().unwrap();
        assert!(matches!(chunk, StreamingChunk::RawSSE(_)));
    }

    // AnthropicEvent tests

    #[test]
    fn test_anthropic_event_parse() {
        let data = b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3\"}}";
        let event = AnthropicEvent::parse(data);
        assert!(matches!(event, Some(AnthropicEvent::MessageStart { .. })));
    }

    #[test]
    fn test_anthropic_event_parse_content_block_start() {
        let data = b"data: {\"type\":\"content_block_start\",\"index\":0}";
        let event = AnthropicEvent::parse(data);
        assert!(matches!(event, Some(AnthropicEvent::ContentBlockStart { index: 0 })));
    }

    #[test]
    fn test_anthropic_event_parse_content_block_delta() {
        let data = b"data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"text\":\"Hello\"}}";
        let event = AnthropicEvent::parse(data);
        match event {
            Some(AnthropicEvent::ContentBlockDelta { index, text }) => {
                assert_eq!(index, 0);
                assert_eq!(text, "Hello");
            }
            _ => panic!("Expected ContentBlockDelta"),
        }
    }

    #[test]
    fn test_anthropic_event_parse_content_block_stop() {
        let data = b"data: {\"type\":\"content_block_stop\",\"index\":0}";
        let event = AnthropicEvent::parse(data);
        assert!(matches!(event, Some(AnthropicEvent::ContentBlockStop { index: 0 })));
    }

    #[test]
    fn test_anthropic_event_parse_message_delta() {
        let data = b"data: {\"type\":\"message_delta\",\"delta\":{\"tokens\":10,\"stop_reason\":\"end_turn\"}}";
        let event = AnthropicEvent::parse(data);
        match event {
            Some(AnthropicEvent::MessageDelta { tokens, stop_reason }) => {
                assert_eq!(tokens, 10);
                assert_eq!(stop_reason, "end_turn");
            }
            _ => panic!("Expected MessageDelta"),
        }
    }

    #[test]
    fn test_anthropic_event_parse_message_stop() {
        let data = b"data: {\"type\":\"message_stop\"}";
        let event = AnthropicEvent::parse(data);
        assert!(matches!(event, Some(AnthropicEvent::MessageStop)));
    }

    #[test]
    fn test_anthropic_event_parse_unknown_type() {
        let data = b"data: {\"type\":\"unknown_event\"}";
        assert!(AnthropicEvent::parse(data).is_none());
    }

    #[test]
    fn test_anthropic_event_parse_invalid_json() {
        let data = b"data: not-json";
        assert!(AnthropicEvent::parse(data).is_none());
    }

    #[test]
    fn test_anthropic_event_parse_no_data_prefix() {
        let data = b"event: message_start";
        assert!(AnthropicEvent::parse(data).is_none());
    }

    #[test]
    fn test_anthropic_event_parse_utf8_error() {
        let data = &[0xFF, 0xFE];
        assert!(AnthropicEvent::parse(data).is_none());
    }

    #[test]
    fn test_anthropic_to_openai_sse() {
        let event = AnthropicEvent::ContentBlockDelta {
            index: 0,
            text: "Hello".to_string(),
        };
        let sse = event.to_openai_sse("msg_123", "claude-3", 1234567890);
        assert!(sse.is_some());
        let sse = sse.unwrap();
        assert!(sse.contains("Hello"));
        assert!(sse.contains("chat.completion.chunk"));
    }

    #[test]
    fn test_anthropic_to_openai_sse_message_delta() {
        let event = AnthropicEvent::MessageDelta {
            tokens: 5,
            stop_reason: "end_turn".to_string(),
        };
        let sse = event.to_openai_sse("msg_123", "claude-3", 1234567890);
        assert!(sse.is_some());
        let sse = sse.unwrap();
        assert!(sse.contains("end_turn"));
    }

    #[test]
    fn test_anthropic_to_openai_sse_message_stop() {
        let event = AnthropicEvent::MessageStop;
        let sse = event.to_openai_sse("msg_123", "claude-3", 1234567890);
        assert!(sse.is_some());
        assert_eq!(sse.unwrap(), "data: [DONE]\n\n");
    }

    #[test]
    fn test_anthropic_to_openai_sse_message_start_returns_none() {
        let event = AnthropicEvent::MessageStart {
            id: "msg_123".into(),
            model: "claude-3".into(),
        };
        assert!(event.to_openai_sse("msg_123", "claude-3", 1234567890).is_none());
    }

    #[test]
    fn test_anthropic_to_openai_sse_content_block_start_returns_none() {
        let event = AnthropicEvent::ContentBlockStart { index: 0 };
        assert!(event.to_openai_sse("msg_123", "claude-3", 1234567890).is_none());
    }

    #[test]
    fn test_anthropic_to_openai_sse_content_block_stop_returns_none() {
        let event = AnthropicEvent::ContentBlockStop { index: 0 };
        assert!(event.to_openai_sse("msg_123", "claude-3", 1234567890).is_none());
    }

    #[test]
    fn test_anthropic_to_openai_sse_text_with_quotes() {
        let event = AnthropicEvent::ContentBlockDelta {
            index: 0,
            text: r#"She said "hello""#.to_string(),
        };
        let sse = event.to_openai_sse("msg_1", "claude-3", 1).unwrap();
        assert!(sse.contains("She said \\\"hello\\\""));
    }

    #[test]
    fn test_anthropic_to_openai_sse_text_with_newlines() {
        let event = AnthropicEvent::ContentBlockDelta {
            index: 0,
            text: "line1\nline2".to_string(),
        };
        let sse = event.to_openai_sse("msg_1", "claude-3", 1).unwrap();
        assert!(sse.contains("line1\\nline2"));
    }

    #[test]
    fn test_anthropic_event_debug() {
        let event = AnthropicEvent::MessageStop;
        assert!(format!("{:?}", event).contains("MessageStop"));
    }

    #[test]
    fn test_anthropic_event_clone() {
        let event = AnthropicEvent::ContentBlockDelta {
            index: 0,
            text: "Hi".to_string(),
        };
        let cloned = event.clone();
        match cloned {
            AnthropicEvent::ContentBlockDelta { text, .. } => assert_eq!(text, "Hi"),
            _ => panic!("Clone failed"),
        }
    }

}
