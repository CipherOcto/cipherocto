// anthropic — Anthropic via reqwest (native_http, LiteLLM mode)
//
// Per RFC-0917 lines 3185-3190: Anthropic SSE must be converted to OpenAI SSE format.

use super::{HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse, ProviderError, StreamingChunk, StreamingResponse};
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
            "claude-3-5-sonnet-latest", "claude-3-5-sonnet-20241022",
            "claude-3-opus-latest", "claude-3-opus-20240229",
            "claude-3-sonnet-latest", "claude-3-sonnet-20240229",
            "claude-3-haiku-latest", "claude-3-haiku-20240307",
        ]
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let url = format!("{}/messages", self.api_base);

        // Convert messages to Anthropic format
        let system = request.messages.iter()
            .filter(|m| m.role == "system")
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let messages: Vec<_> = request.messages.iter()
            .filter(|m| m.role != "system")
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
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

        let resp = self.client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
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
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!("HTTP {}: {}", status, text)));
        }

        let data: AnthropicResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpCompletionResponse {
            id: format!("msg_{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new("assistant", data.content.first().and_then(|c| c.text.as_ref()).unwrap_or(&String::new()).clone()),
                data.stop_reason.unwrap_or_else(|| "stop".to_string()),
            )],
            usage: crate::shared_types::Usage::new(data.usage.input_tokens, data.usage.output_tokens, data.usage.input_tokens + data.usage.output_tokens),
        })
    }

    async fn embedding(
        &self,
        _request: &HttpEmbeddingRequest,
        _api_key: &str,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel("Anthropic does not support embeddings".to_string()))
    }

    fn routing_weight(&self) -> u32 {
        8
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<StreamingResponse, ProviderError> {
        let url = format!("{}/messages", self.api_base);

        let system = request.messages.iter()
            .filter(|m| m.role == "system")
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let messages: Vec<_> = request.messages.iter()
            .filter(|m| m.role != "system")
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

        let resp = self.client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
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
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!("HTTP {}: {}", status, text)));
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
                            if let Some(openai_sse) = event.to_openai_sse(&chunk_id, &model, created) {
                                if tx.send(Ok(StreamingChunk::RawSSE(openai_sse.into_bytes()))).await.is_err() {
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
    #[allow(dead_code)]
    r#type: String,
    text: Option<String>,
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
            "content_block_start" => {
                Some(AnthropicEvent::ContentBlockStart {
                    index: json.get("index")?.as_u64()? as u32,
                })
            }
            "content_block_delta" => {
                let delta = json.get("delta")?;
                Some(AnthropicEvent::ContentBlockDelta {
                    index: json.get("index")?.as_u64()? as u32,
                    text: delta.get("text")?.as_str()?.to_string(),
                })
            }
            "content_block_stop" => {
                Some(AnthropicEvent::ContentBlockStop {
                    index: json.get("index")?.as_u64()? as u32,
                })
            }
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

    #[test]
    fn test_anthropic_event_parse() {
        let data = b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3\"}}";
        let event = AnthropicEvent::parse(data);
        assert!(matches!(event, Some(AnthropicEvent::MessageStart { .. })));
    }

    #[test]
    fn test_anthropic_to_openai_sse() {
        let event = AnthropicEvent::ContentBlockDelta { index: 0, text: "Hello".to_string() };
        let sse = event.to_openai_sse("msg_123", "claude-3", 1234567890);
        assert!(sse.is_some());
        let sse = sse.unwrap();
        assert!(sse.contains("Hello"));
        assert!(sse.contains("chat.completion.chunk"));
    }
}
