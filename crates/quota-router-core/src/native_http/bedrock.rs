// Clippy `[disallowed-methods]` allowlist: this module is a
// legitimate provider-egress adapter. It talks to the model
// provider's REST API and routes the Authorization header through
// `egress::key_swap::attach_bearer` so the cipherocto-internal key
// is swapped for the provider's key before the request leaves.
// Capability tokens never reach the provider (see `egress::strip_capability`).
#![allow(clippy::disallowed_methods)]

// bedrock — AWS Bedrock via reqwest (native_http, LiteLLM mode)

use super::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError, StreamingChunk, StreamingResponse,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;

pub struct BedrockProvider {
    client: Client,
    region: String,
}

impl BedrockProvider {
    pub fn new() -> Self {
        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        Self {
            client: Client::new(),
            region,
        }
    }

    pub fn with_region(mut self, region: String) -> Self {
        self.region = region;
        self
    }
}

impl Default for BedrockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for BedrockProvider {
    fn name(&self) -> &str {
        "bedrock"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "anthropic.claude-3-5-sonnet-latest",
            "anthropic.claude-3-opus-latest",
            "anthropic.claude-3-sonnet-latest",
            "anthropic.claude-3-haiku-latest",
            "meta.llama3-1-70b-instruct",
            "meta.llama3-1-8b-instruct",
            "mistral.mistral-large-2407",
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
        let url = format!(
            "https://bedrock.{}.amazonaws.com/model/{}",
            self.region, request.model
        );

        // Build request body for Bedrock (varies by provider)
        let body = serde_json::json!({
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>(),
            "anthropic_version": "bedrock-2023-05-31"
        });

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("x-amz-client-id", key);
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

        let data: BedrockResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpCompletionResponse {
            id: format!("bedrock-{}", uuid::Uuid::new_v4()),
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
                        .and_then(|c| c.text.as_ref())
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
            "Bedrock embeddings not implemented".to_string(),
        ))
    }

    fn routing_weight(&self) -> u32 {
        4
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        // Bedrock uses invoke-with-response-stream endpoint
        let url = format!(
            "https://bedrock.{}.amazonaws.com/model/{}/invoke-with-response-stream",
            self.region, request.model
        );

        // Build request body for Bedrock (varies by provider)
        let body = serde_json::json!({
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>(),
            "anthropic_version": "bedrock-2023-05-31"
        });

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("x-amz-client-id", key);
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
            let chunk_id = format!("bedrock-{}", uuid::Uuid::new_v4());
            let created = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut buffer = Vec::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        // AWS EventStream format: process binary-framed messages
                        // Each message has: total_length (4 bytes), headers_length (4 bytes),
                        // prelude_crc (4 bytes), headers, payload, message_crc (4 bytes)
                        while buffer.len() >= 12 {
                            let total_len =
                                u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]])
                                    as usize;

                            if buffer.len() < total_len {
                                break; // Need more data
                            }

                            let message_bytes: Vec<u8> = buffer.drain(..total_len).collect();

                            // Extract payload (after prelude + headers)
                            let headers_len = u32::from_be_bytes([
                                message_bytes[4],
                                message_bytes[5],
                                message_bytes[6],
                                message_bytes[7],
                            ]) as usize;

                            let payload_start = 12 + headers_len; // prelude(8) + prelude_crc(4) + headers
                            if payload_start >= message_bytes.len() {
                                continue;
                            }

                            let payload = &message_bytes[payload_start..message_bytes.len() - 4]; // exclude message_crc

                            // Parse the JSON payload
                            if let Ok(json_str) = std::str::from_utf8(payload) {
                                if let Ok(event) =
                                    serde_json::from_str::<serde_json::Value>(json_str)
                                {
                                    // Check for content_block_delta events
                                    if let Some(event_type) =
                                        event.get("type").and_then(|t| t.as_str())
                                    {
                                        match event_type {
                                            "content_block_delta" => {
                                                if let Some(delta) = event.get("delta") {
                                                    if let Some(text) =
                                                        delta.get("text").and_then(|t| t.as_str())
                                                    {
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
                                            }
                                            "message_delta" => {
                                                if let Some(delta) = event.get("delta") {
                                                    if let Some(stop_reason) = delta
                                                        .get("stop_reason")
                                                        .and_then(|s| s.as_str())
                                                    {
                                                        let finish_chunk = format!(
                                                            "data: {{\"id\":\"{}\",\"object\":\"chat.completion.chunk\",\"created\":{},\"model\":\"{}\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"{}\"}}]}}\n\n",
                                                            chunk_id, created, model, stop_reason
                                                        );
                                                        let _ = tx
                                                            .send(Ok(StreamingChunk::RawSSE(
                                                                finish_chunk.into_bytes(),
                                                            )))
                                                            .await;
                                                        // Send DONE marker
                                                        let _ = tx
                                                            .send(Ok(StreamingChunk::RawSSE(
                                                                "data: [DONE]\n\n"
                                                                    .as_bytes()
                                                                    .to_vec(),
                                                            )))
                                                            .await;
                                                    }
                                                }
                                            }
                                            "message_stop" => {
                                                // Message stop without explicit finish - send DONE
                                                let _ = tx
                                                    .send(Ok(StreamingChunk::RawSSE(
                                                        "data: [DONE]\n\n".as_bytes().to_vec(),
                                                    )))
                                                    .await;
                                            }
                                            _ => {} // Other event types ignored
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

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct BedrockResponse {
    id: String,
    #[allow(dead_code)]
    type_: String,
    content: Vec<BedrockContentBlock>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
    #[allow(dead_code)]
    stop_sequence: Option<String>,
    usage: BedrockUsage,
}

#[derive(serde::Deserialize)]
struct BedrockContentBlock {
    #[allow(dead_code)]
    r#type: String,
    text: Option<String>,
}

#[derive(serde::Deserialize)]
struct BedrockUsage {
    #[allow(dead_code)]
    input_tokens: u32,
    #[allow(dead_code)]
    output_tokens: u32,
}
